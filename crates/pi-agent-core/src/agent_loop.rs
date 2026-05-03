use pi_ai::{
    AssistantMessageEvent, ContentBlock, Context as LlmContext, Message, StopReason, Tool, now_ms,
};

use serde_json::Value;
use tokio::sync::watch;

use crate::types::{
    AfterToolCallContext, AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, AgentTool,
    AgentToolResult, BeforeToolCallContext, StreamFn, ToolExecutionMode,
};

pub async fn stream_assistant_response(
    context: &mut AgentContext,
    config: &dyn AgentLoopConfig,
    signal: Option<&watch::Receiver<bool>>,
    emit: &mut dyn FnMut(AgentEvent),
    stream_fn: &dyn StreamFn,
) -> Message {
    let transformed = config.transform_context(&context.messages, signal).await;

    let llm_messages = config.convert_to_llm(&transformed).await;

    let tools = context.tools.as_ref().map(|tools| {
        tools
            .iter()
            .map(|t| Tool {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters().clone(),
            })
            .collect()
    });

    let llm_context = LlmContext {
        system_prompt: Some(context.system_prompt.clone()),
        messages: llm_messages,
        tools,
    };

    let resolved_api_key = config
        .get_api_key(&config.model().provider)
        .await
        .or_else(|| config.stream_options().api_key.clone());

    let mut options = config.stream_options();
    options.api_key = resolved_api_key;

    let mut stream = stream_fn.stream(config.model(), &llm_context, options);

    let mut partial_message: Option<Message> = None;
    let mut added_partial = false;

    // Main Agent Event Loop
    while let Some(event) = stream.next().await {
        match event {
            // Handle Event
            AssistantMessageEvent::Start { partial } => {
                partial_message = Some(partial.clone());
                context.messages.push(AgentMessage::from(partial.clone()));
                added_partial = true;
                emit(AgentEvent::MessageStart {
                    message: AgentMessage::from(partial.clone()),
                });
            }
            AssistantMessageEvent::TextStart { ref partial, .. }
            | AssistantMessageEvent::TextDelta { ref partial, .. }
            | AssistantMessageEvent::TextEnd { ref partial, .. }
            | AssistantMessageEvent::ThinkingStart { ref partial, .. }
            | AssistantMessageEvent::ThinkingDelta { ref partial, .. }
            | AssistantMessageEvent::ThinkingEnd { ref partial, .. }
            | AssistantMessageEvent::ToolCallStart { ref partial, .. }
            | AssistantMessageEvent::ToolCallDelta { ref partial, .. }
            | AssistantMessageEvent::ToolCallEnd { ref partial, .. } => {
                if let Some(ref mut pm) = partial_message {
                    *pm = partial.clone();
                    if let Some(last) = context.messages.last_mut() {
                        *last = AgentMessage::from(partial.clone());
                    }
                    emit(AgentEvent::MessageUpdate {
                        message: AgentMessage::from(partial.clone()),
                        assistant_message_event: Box::new(event),
                    })
                }
            }

            // Done
            AssistantMessageEvent::Done { message, .. }
            | AssistantMessageEvent::Error { error: message, .. } => {
                if added_partial {
                    if let Some(last) = context.messages.last_mut() {
                        *last = AgentMessage::from(message.clone());
                    } else {
                        context.messages.push(AgentMessage::from(message.clone()));
                    }
                    if !added_partial {
                        emit(AgentEvent::MessageStart {
                            message: AgentMessage::from(message.clone()),
                        });
                    }
                    emit(AgentEvent::MessageEnd {
                        message: AgentMessage::from(message.clone()),
                    });
                    return message;
                }
            }
        }
    }

    if let Some(message) = partial_message {
        if !added_partial {
            emit(AgentEvent::MessageStart {
                message: AgentMessage::from(message.clone()),
            });
        }
        emit(AgentEvent::MessageEnd {
            message: AgentMessage::from(message.clone()),
        });
        return message;
    }

    Message::Assistant {
        content: vec![],
        api: String::new(),
        provider: String::new(),
        model: String::new(),
        response_id: None,
        usage: pi_ai::Usage::default(),
        stop_reason: pi_ai::StopReason::Error,
        error_message: Some("Stream ended without response".into()),
        timestamp: 0,
    }
}

pub async fn run_agent_loop(
    prompts: Vec<AgentMessage>,
    context: &mut AgentContext,
    config: &dyn AgentLoopConfig,
    signal: Option<&watch::Receiver<bool>>,
    emit: &mut dyn FnMut(AgentEvent),
    stream_fn: &dyn StreamFn,
) -> Vec<AgentMessage> {
    // main entry point of agent loop
    let mut new_messages = prompts.clone();
    context.messages.extend(prompts);

    // Emit AgentStart & TurnStart
    emit(AgentEvent::AgentStart);
    emit(AgentEvent::TurnStart);

    for prompt in &new_messages {
        emit(AgentEvent::MessageStart {
            message: prompt.clone(),
        });
        emit(AgentEvent::MessageEnd {
            message: prompt.clone(),
        });
    }
    // inner run_loop
    run_loop(context, config, &mut new_messages, signal, emit, stream_fn).await;

    new_messages
}

pub async fn run_agent_loop_continue(
    context: &mut AgentContext,
    config: &dyn AgentLoopConfig,
    signal: Option<&watch::Receiver<bool>>,
    emit: &mut dyn FnMut(AgentEvent),
    stream_fn: &dyn StreamFn,
) -> Vec<AgentMessage> {
    if context.messages.is_empty() {
        panic!("No meesage in context");
    }
    if matches!(
        context.messages.last(),
        Some(AgentMessage::Assistant { .. })
    ) {
        panic!("Cannot continue from message role: Assistant");
    }

    let mut new_messages = Vec::new();
    emit(AgentEvent::AgentStart);
    emit(AgentEvent::TurnStart);

    run_loop(context, config, &mut new_messages, signal, emit, stream_fn).await;

    new_messages
}

struct ExecutedToolCallBatch {
    messages: Vec<AgentMessage>,
    terminate: bool,
}

struct FinalizedToolCallOutcome {
    tool_call: ContentBlock,
    result: AgentToolResult,
    is_error: bool,
}

struct ExecutedToolCallOutcome {
    result: AgentToolResult,
    is_error: bool,
}

fn create_error_tool_result(message: impl Into<String>) -> AgentToolResult {
    AgentToolResult {
        content: vec![ContentBlock::Text {
            text: message.into(),
            text_signature: None,
        }],
        details: Value::Null,
        terminate: false,
    }
}

fn should_terminate_tool_batch(finalized_calls: &[FinalizedToolCallOutcome]) -> bool {
    !finalized_calls.is_empty() && finalized_calls.iter().all(|f| f.result.terminate)
}

fn emit_tool_execution_end(finalized: &FinalizedToolCallOutcome, emit: &mut dyn FnMut(AgentEvent)) {
    if let ContentBlock::ToolCall { id, name, .. } = &finalized.tool_call {
        emit(AgentEvent::ToolExecutionEnd {
            tool_call_id: id.clone(),
            tool_name: name.clone(),
            result: finalized.result.details.clone(),
            is_error: finalized.is_error,
        })
    }
}

fn create_tool_result_message(finalized: &FinalizedToolCallOutcome) -> AgentMessage {
    if let ContentBlock::ToolCall { id, name, .. } = &finalized.tool_call {
        AgentMessage::ToolResult {
            tool_call_id: id.clone(),
            tool_name: name.clone(),
            content: finalized.result.content.clone(),
            is_error: finalized.is_error,
            timestamp: now_ms(),
        }
    } else {
        panic!("Expected ToolCall content block")
    }
}

fn emit_tool_result_message(message: &AgentMessage, emit: &mut dyn FnMut(AgentEvent)) {
    emit(AgentEvent::MessageStart {
        message: message.clone(),
    });
    emit(AgentEvent::MessageEnd {
        message: message.clone(),
    });
}

async fn run_loop(
    context: &mut AgentContext,
    config: &dyn AgentLoopConfig,
    new_messages: &mut Vec<AgentMessage>,
    signal: Option<&watch::Receiver<bool>>,
    emit: &mut dyn FnMut(AgentEvent),
    stream_fn: &dyn StreamFn,
) {
    // Core Agent Loop
    let mut first_run = true;
    let mut pending_messages = config.get_steering_messages().await;

    loop {
        let mut has_more_tool_calls = true;

        while has_more_tool_calls || !pending_messages.is_empty() {
            if !first_run {
                emit(AgentEvent::TurnStart);
            } else {
                first_run = false;
            }

            // handle pending messages
            for message in pending_messages.drain(..) {
                emit(AgentEvent::MessageStart {
                    message: message.clone(),
                });
                emit(AgentEvent::MessageEnd {
                    message: message.clone(),
                });
                context.messages.push(message.clone());
                new_messages.push(message);
            }

            // Call LLM
            let message = stream_assistant_response(context, config, signal, emit, stream_fn).await;
            let agent_message = AgentMessage::from(message.clone());
            new_messages.push(agent_message.clone());

            // handle error or interrupt
            if let Message::Assistant { stop_reason, .. } = &message {
                if *stop_reason == StopReason::Error || *stop_reason == StopReason::Aborted {
                    emit(AgentEvent::TurnEnd {
                        message: agent_message,
                        tool_results: vec![],
                    });
                    emit(AgentEvent::AgentEnd {
                        messages: new_messages.clone(),
                    });
                    return;
                }
            }

            // Check Tool Calls
            let tool_calls: Vec<&ContentBlock> =
                if let Message::Assistant { content, .. } = &message {
                    content
                        .iter()
                        .filter(|c| matches!(c, ContentBlock::ToolCall { .. }))
                        .collect()
                } else {
                    vec![]
                };

            let mut tool_results: Vec<AgentMessage> = vec![];
            has_more_tool_calls = false;

            if !tool_calls.is_empty() {
                let executed_batch =
                    execute_tool_calls(context, &message, config, signal, emit).await;
                tool_results = executed_batch.messages;
                has_more_tool_calls = !executed_batch.terminate;

                for result in &tool_results {
                    context.messages.push(result.clone());
                    new_messages.push(result.clone());
                }
            }
            emit(AgentEvent::TurnEnd {
                message: agent_message,
                tool_results,
            });
            pending_messages = config.get_steering_messages().await;
        }

        // end of inner loop, check follow-up message
        let follow_up_message = config.get_follow_up_messages().await;
        if !follow_up_message.is_empty() {
            pending_messages = follow_up_message;
            continue;
        }

        break;
    }

    emit(AgentEvent::AgentEnd {
        messages: new_messages.clone(),
    });
}

enum ToolCallPreparation<'a> {
    Prepared {
        tool: &'a dyn AgentTool,
        args: Value,
    },
    Immediate {
        result: AgentToolResult,
        is_error: bool,
    },
}

async fn prepare_tool_call<'a>(
    context: &'a AgentContext,
    assistant_message: &Message,
    tool_call: &'a ContentBlock,
    config: &dyn AgentLoopConfig,
    signal: Option<&watch::Receiver<bool>>,
) -> ToolCallPreparation<'a> {
    let (tool_name, args) = if let ContentBlock::ToolCall {
        name, arguments, ..
    } = tool_call
    {
        (name, arguments)
    } else {
        return ToolCallPreparation::Immediate {
            result: create_error_tool_result("Expected tool call content block"),
            is_error: true,
        };
    };

    let tool = context
        .tools
        .as_ref()
        .and_then(|tools| tools.iter().find(|t| t.name() == *tool_name));

    let tool = match tool {
        Some(t) => t,
        None => {
            return ToolCallPreparation::Immediate {
                result: create_error_tool_result(format!("Tool {} not found", tool_name)),
                is_error: true,
            };
        }
    };

    let prepared_args = tool.prepare_arguments(args.clone());
    let before_result = config
        .before_tool_call(
            BeforeToolCallContext {
                assistant_message,
                tool_call,
                args: prepared_args.clone(),
                context,
            },
            signal,
        )
        .await;

    if let Some(before) = before_result
        && before.block
    {
        return ToolCallPreparation::Immediate {
            result: create_error_tool_result(
                before
                    .reason
                    .unwrap_or_else(|| "Tool execution was blocked".into()),
            ),
            is_error: true,
        };
    }

    ToolCallPreparation::Prepared {
        tool: tool.as_ref(),
        args: prepared_args,
    }
}

async fn execute_tool_calls(
    context: &mut AgentContext,
    assistant_message: &Message,
    config: &dyn AgentLoopConfig,
    signal: Option<&watch::Receiver<bool>>,
    emit: &mut dyn FnMut(AgentEvent),
) -> ExecutedToolCallBatch {
    let tool_calls = if let Message::Assistant { content, .. } = assistant_message {
        content
            .iter()
            .filter(|c| matches!(c, ContentBlock::ToolCall { .. }))
            .collect()
    } else {
        vec![]
    };

    let has_sequential = tool_calls.iter().any(|tc| {
        if let ContentBlock::ToolCall { name, .. } = tc {
            context
                .tools
                .as_ref()
                .and_then(|tools| tools.iter().find(|t| t.name() == *name))
                .map(|t| t.execution_mode() == ToolExecutionMode::Sequential)
                .unwrap_or(false)
        } else {
            false
        }
    });

    if config.tool_execution() == ToolExecutionMode::Sequential || has_sequential {
        execute_tool_calls_sequential(
            context,
            assistant_message,
            &tool_calls,
            config,
            signal,
            emit,
        )
        .await
    } else {
        // TODO: parallel version
        todo!()
    }
}

async fn execute_tool_calls_sequential(
    context: &mut AgentContext,
    assistant_message: &Message,
    tool_calls: &[&ContentBlock],
    config: &dyn AgentLoopConfig,
    signal: Option<&watch::Receiver<bool>>,
    emit: &mut dyn FnMut(AgentEvent),
) -> ExecutedToolCallBatch {
    let mut finalized_calls: Vec<FinalizedToolCallOutcome> = vec![];
    let mut messages: Vec<AgentMessage> = vec![];

    for &tool_call in tool_calls {
        if let ContentBlock::ToolCall {
            id,
            name,
            arguments,
            ..
        } = tool_call
        {
            emit(AgentEvent::ToolExecutionStart {
                tool_call_id: id.clone(),
                tool_name: name.clone(),
                args: arguments.clone(),
            });
            let preparation =
                prepare_tool_call(context, assistant_message, tool_call, config, signal).await;
            let finalized = match preparation {
                ToolCallPreparation::Immediate { result, is_error } => FinalizedToolCallOutcome {
                    tool_call: tool_call.clone(),
                    result,
                    is_error,
                },
                ToolCallPreparation::Prepared { tool, args, .. } => {
                    let executed = execute_prepared_tool_call(tool, id, &args, signal).await;
                    finalize_executed_tool_call(
                        context,
                        assistant_message,
                        tool_call,
                        args,
                        executed,
                        config,
                        signal,
                    )
                    .await
                }
            };

            emit_tool_execution_end(&finalized, emit);
            let tool_result_message = create_tool_result_message(&finalized);
            emit_tool_result_message(&tool_result_message, emit);
            finalized_calls.push(finalized);
            messages.push(tool_result_message);
        }
    }

    ExecutedToolCallBatch {
        messages,
        terminate: should_terminate_tool_batch(&finalized_calls),
    }
}

async fn execute_prepared_tool_call(
    tool: &dyn AgentTool,
    tool_call_id: &str,
    args: &Value,
    signal: Option<&watch::Receiver<bool>>,
) -> ExecutedToolCallOutcome {
    let result = tool.execute(tool_call_id, args.clone(), signal, None).await;

    ExecutedToolCallOutcome {
        result,
        is_error: false,
    }
}

async fn finalize_executed_tool_call<'a>(
    context: &'a AgentContext,
    assistant_message: &Message,
    tool_call: &ContentBlock,
    prepared_args: Value,
    executed: ExecutedToolCallOutcome,
    config: &dyn AgentLoopConfig,
    signal: Option<&watch::Receiver<bool>>,
) -> FinalizedToolCallOutcome {
    let mut result = executed.result;
    let mut is_error = executed.is_error;

    let after_result = config
        .after_tool_call(
            AfterToolCallContext {
                assistant_message,
                tool_call,
                args: prepared_args,
                result: result.clone(),
                is_error,
                context,
            },
            signal,
        )
        .await;

    if let Some(after) = after_result {
        if let Some(content) = after.content {
            result.content = content;
        }
        if let Some(details) = after.details {
            result.details = details;
        }
        if let Some(err) = after.is_error {
            is_error = err;
        }
        if let Some(terminate) = after.terminate {
            result.terminate = terminate;
        }
    }

    FinalizedToolCallOutcome {
        tool_call: tool_call.clone(),
        result,
        is_error,
    }
}
