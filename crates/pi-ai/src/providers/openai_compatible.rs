use std::collections::HashMap;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use tokio::io::AsyncBufReadExt;
use tokio_stream::StreamExt;
use tokio_util::io::StreamReader;

use crate::{
    config::OpenAICompatibleConfig,
    error::AIError,
    message::{ContentBlock, Message, StopReason, UserContent},
    provider::ApiProvider,
    stream::{AssistantMessageEvent, AssistantMessageEventStream},
    types::{Context, GenerateResponse, Model, StreamOptions, Usage},
};

#[derive(Clone)]
pub struct OpenAICompatibleProvider {
    config: OpenAICompatibleConfig,
    client: Client,
}

impl OpenAICompatibleProvider {
    pub fn new(config: OpenAICompatibleConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    pub async fn generate_direct(
        &self,
        model: &str,
        messages: Vec<Message>,
        options: &StreamOptions,
    ) -> Result<GenerateResponse, AIError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let body = OpenAICompletionRequest {
            model: model.to_string(),
            messages: messages.into_iter().filter_map(message_to_openai).collect(),
            stream: false,
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            top_p: options.top_p,
            frequency_penalty: options.frequency_penalty,
            presence_penalty: options.presence_penalty,
            stop: options.stop.clone(),
            tools: None,
        };

        let resp = self
            .client
            .post(url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|err| AIError::Provider(format!("request failed: {err}")))?;

        let status = resp.status();

        if !status.is_success() {
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "failed to read error body".to_string());

            return Err(AIError::Provider(format!(
                "provider returned {}: {}",
                status, text
            )));
        }

        let data: OpenAIChatCompletionResponse = resp
            .json()
            .await
            .map_err(|err| AIError::Provider(format!("invalid response json: {err}")))?;

        let first_choice = data
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AIError::Provider("response choices is empty".to_string()))?;

        Ok(GenerateResponse {
            model: data.model,
            content: first_choice.message.content,
            usage: data.usage.map(|u| Usage {
                input: u.prompt_tokens as u64,
                output: u.completion_tokens as u64,
                cache_read: 0,
                cache_write: 0,
                total_tokens: u.total_tokens as u64,
                cost: crate::types::Cost::default(),
            }),
            finish_reason: first_choice.finish_reason,
        })
    }
}

#[derive(Debug, Serialize)]
struct OpenAICompletionRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    stream: bool,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    top_p: Option<f32>,
    frequency_penalty: Option<f32>,
    presence_penalty: Option<f32>,
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCallDelta {
    index: Option<usize>,
    id: Option<String>,
    #[serde(rename = "type")]
    call_type: Option<String>,
    function: Option<OpenAIFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAIFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIDelta {
    content: Option<String>,
    #[serde(rename = "reasoning_content")]
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    id: String,
    model: String,
    choices: Vec<OpenAIStreamChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChatCompletionResponse {
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIAssistantMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIAssistantMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

fn message_to_openai(m: Message) -> Option<OpenAIMessage> {
    match m {
        Message::User { content, .. } => {
            let text = match content {
                UserContent::Plain(s) => s,
                UserContent::Blocks(blocks) => blocks
                    .into_iter()
                    .map(|b| match b {
                        ContentBlock::Text { text, .. } => text,
                        _ => String::new(),
                    })
                    .collect(),
            };
            Some(OpenAIMessage {
                role: "user".to_string(),
                content: text,
            })
        }
        Message::Assistant { content, .. } => {
            let text = content
                .into_iter()
                .map(|b| match b {
                    ContentBlock::Text { text, .. } => text,
                    _ => String::new(),
                })
                .collect::<String>();
            Some(OpenAIMessage {
                role: "assistant".to_string(),
                content: text,
            })
        }
        Message::ToolResult { content, .. } => {
            let text = content
                .into_iter()
                .map(|b| match b {
                    ContentBlock::Text { text, .. } => text,
                    _ => String::new(),
                })
                .collect::<String>();
            Some(OpenAIMessage {
                role: "tool".to_string(),
                content: text,
            })
        }
    }
}

fn tools_to_openai(tools: &Option<Vec<crate::types::Tool>>) -> Option<Vec<OpenAITool>> {
    tools.as_ref().map(|ts| {
        ts.iter()
            .map(|t| OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    })
}

impl ApiProvider for OpenAICompatibleProvider {
    fn api(&self) -> &'static str {
        "openai-completions"
    }

    fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: StreamOptions,
    ) -> AssistantMessageEventStream {
        let (stream, handle) = AssistantMessageEventStream::new();
        let provider = self.clone();
        let model_id = model.id.clone();
        let messages = context.messages.clone();
        let tools = context.tools.clone();

        tokio::spawn(async move {
            let url = format!(
                "{}/chat/completions",
                provider.config.base_url.trim_end_matches("/")
            );

            let body = OpenAICompletionRequest {
                model: model_id.clone(),
                messages: messages.into_iter().filter_map(message_to_openai).collect(),
                stream: true,
                temperature: options.temperature,
                max_tokens: options.max_tokens,
                top_p: options.top_p,
                frequency_penalty: options.frequency_penalty,
                presence_penalty: options.presence_penalty,
                stop: options.stop,
                tools: tools_to_openai(&tools),
            };

            let resp = match provider
                .client
                .post(url)
                .bearer_auth(&provider.config.api_key)
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(err) => {
                    let error_msg = Message::Assistant {
                        content: vec![],
                        api: "openai-compatible".to_string(),
                        provider: "openai-compatible".to_string(),
                        model: model_id.clone(),
                        response_id: None,
                        usage: Usage::default(),
                        stop_reason: StopReason::Error,
                        error_message: Some(format!("request failed: {err}")),
                        timestamp: 0,
                    };
                    handle.push(AssistantMessageEvent::Error {
                        reason: StopReason::Error,
                        error: error_msg,
                    });
                    return;
                }
            };

            let status = resp.status();
            if !status.is_success() {
                let text = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "unknown error".to_string());
                let error_msg = Message::Assistant {
                    content: vec![],
                    api: "openai-compatible".to_string(),
                    provider: "openai-compatible".to_string(),
                    model: model_id.clone(),
                    response_id: None,
                    usage: Usage::default(),
                    stop_reason: StopReason::Error,
                    error_message: Some(format!("provider returned {}: {}", status, text)),
                    timestamp: 0,
                };
                handle.push(AssistantMessageEvent::Error {
                    reason: StopReason::Error,
                    error: error_msg,
                });
                return;
            }

            let byte_stream = resp
                .bytes_stream()
                .map(|result| result.map_err(|e| std::io::Error::other(e)));
            let reader = StreamReader::new(byte_stream);
            let mut lines = reader.lines();

            handle.push(AssistantMessageEvent::Start {
                partial: Message::Assistant {
                    content: vec![],
                    api: "openai-compatible".to_string(),
                    provider: "openai-compatible".to_string(),
                    model: model_id.clone(),
                    response_id: None,
                    usage: Usage::default(),
                    stop_reason: StopReason::Stop,
                    error_message: None,
                    timestamp: 0,
                },
            });

            let mut accumulated_text = String::new();
            let mut accumulated_reasoning = String::new();
            let mut has_thinking_started = false;
            let mut partial_tool_calls: HashMap<usize, PartialToolCall> = HashMap::new();
            let mut has_tool_call_started: HashMap<usize, bool> = HashMap::new();
            let mut final_stop_reason = StopReason::Stop;
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim_end();
                if line.is_empty() || !line.starts_with("data: ") {
                    continue;
                }
                let data = &line["data: ".len()..];
                if data == "[DONE]" {
                    let mut content_blocks: Vec<ContentBlock> = Vec::new();
                    if !accumulated_reasoning.is_empty() {
                        content_blocks.push(ContentBlock::Thinking {
                            thinking: accumulated_reasoning,
                            thinking_signature: None,
                            redacted: None,
                        });
                    }
                    if !accumulated_text.is_empty() {
                        content_blocks.push(ContentBlock::Text {
                            text: accumulated_text,
                            text_signature: None,
                        });
                    }
                    let mut indices: Vec<_> = partial_tool_calls.keys().copied().collect();
                    indices.sort();
                    for idx in indices {
                        if let Some(partial) = partial_tool_calls.remove(&idx) {
                            let args = serde_json::from_str(&partial.arguments)
                                .unwrap_or(serde_json::Value::Null);
                            content_blocks.push(ContentBlock::ToolCall {
                                id: partial.id.unwrap_or_default(),
                                name: partial.name.unwrap_or_default(),
                                arguments: args,
                                thought_signature: None,
                            });
                        }
                    }
                    handle.push(AssistantMessageEvent::Done {
                        reason: final_stop_reason.clone(),
                        message: Message::Assistant {
                            content: content_blocks,
                            api: "openai-compatible".to_string(),
                            provider: "openai-compatible".to_string(),
                            model: model_id.clone(),
                            response_id: None,
                            usage: Usage::default(),
                            stop_reason: final_stop_reason,
                            error_message: None,
                            timestamp: 0,
                        },
                    });
                    return;
                }

                let chunk: OpenAIStreamChunk = match serde_json::from_str(data) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                if let Some(choice) = chunk.choices.first() {
                    if let Some(text) = &choice.delta.content
                        && !text.is_empty()
                    {
                        accumulated_text.push_str(text);

                        let current_partial = build_partial_message(
                            &accumulated_text,
                            &accumulated_reasoning,
                            &partial_tool_calls,
                            &model_id,
                        );

                        let text_idx = if accumulated_reasoning.is_empty() {
                            0
                        } else {
                            1
                        };
                        handle.push(AssistantMessageEvent::TextDelta {
                            content_index: text_idx,
                            delta: text.clone(),
                            partial: current_partial,
                        });
                    }

                    if let Some(reasoning) = &choice.delta.reasoning_content
                        && !reasoning.is_empty()
                    {
                        let is_first = !has_thinking_started;
                        accumulated_reasoning.push_str(reasoning);
                        has_thinking_started = true;

                        let current_partial = build_partial_message(
                            &accumulated_text,
                            &accumulated_reasoning,
                            &partial_tool_calls,
                            &model_id,
                        );

                        if is_first {
                            handle.push(AssistantMessageEvent::ThinkingStart {
                                content_index: 0,
                                partial: current_partial.clone(),
                            });
                        }
                        handle.push(AssistantMessageEvent::ThinkingDelta {
                            content_index: 0,
                            delta: reasoning.clone(),
                            partial: current_partial,
                        });
                    }

                    if let Some(tool_calls) = &choice.delta.tool_calls {
                        for tc in tool_calls {
                            let index = tc.index.unwrap_or(0);
                            let is_first =
                                !has_tool_call_started.get(&index).copied().unwrap_or(false);

                            let has_identifiable_fields = {
                                let partial = partial_tool_calls.entry(index).or_default();
                                if let Some(id) = &tc.id {
                                    partial.id = Some(id.clone());
                                }
                                if let Some(func) = &tc.function {
                                    if let Some(name) = &func.name {
                                        partial.name = Some(name.clone());
                                    }
                                    if let Some(args) = &func.arguments {
                                        partial.arguments.push_str(args);
                                    }
                                }
                                partial.id.is_some() || partial.name.is_some()
                            };

                            let current_partial = build_partial_message(
                                &accumulated_text,
                                &accumulated_reasoning,
                                &partial_tool_calls,
                                &model_id,
                            );

                            let tool_base = if accumulated_reasoning.is_empty() {
                                1
                            } else {
                                2
                            };
                            if is_first && has_identifiable_fields {
                                has_tool_call_started.insert(index, true);
                                handle.push(AssistantMessageEvent::ToolCallStart {
                                    content_index: tool_base + index,
                                    partial: current_partial.clone(),
                                });
                            }

                            if let Some(func) = &tc.function
                                && let Some(args) = &func.arguments
                            {
                                handle.push(AssistantMessageEvent::ToolCallDelta {
                                    content_index: tool_base + index,
                                    delta: args.clone(),
                                    partial: current_partial,
                                });
                            }
                        }
                    }

                    if let Some(reason) = &choice.finish_reason {
                        final_stop_reason = match reason.as_str() {
                            "stop" => StopReason::Stop,
                            "length" => StopReason::Length,
                            "tool_calls" => StopReason::ToolUse,
                            _ => StopReason::Stop,
                        };
                    }
                }
            }

            // Stream ended without [DONE] (EOF or IO error) — push Done as fallback.
            let mut content_blocks: Vec<ContentBlock> = Vec::new();
            if !accumulated_reasoning.is_empty() {
                content_blocks.push(ContentBlock::Thinking {
                    thinking: accumulated_reasoning,
                    thinking_signature: None,
                    redacted: None,
                });
            }
            if !accumulated_text.is_empty() {
                content_blocks.push(ContentBlock::Text {
                    text: accumulated_text,
                    text_signature: None,
                });
            }
            let mut indices: Vec<_> = partial_tool_calls.keys().copied().collect();
            indices.sort();
            for idx in indices {
                if let Some(partial) = partial_tool_calls.remove(&idx) {
                    let args =
                        serde_json::from_str(&partial.arguments).unwrap_or(serde_json::Value::Null);
                    content_blocks.push(ContentBlock::ToolCall {
                        id: partial.id.unwrap_or_default(),
                        name: partial.name.unwrap_or_default(),
                        arguments: args,
                        thought_signature: None,
                    });
                }
            }
            handle.push(AssistantMessageEvent::Done {
                reason: final_stop_reason.clone(),
                message: Message::Assistant {
                    content: content_blocks,
                    api: "openai-compatible".to_string(),
                    provider: "openai-compatible".to_string(),
                    model: model_id.clone(),
                    response_id: None,
                    usage: Usage::default(),
                    stop_reason: final_stop_reason,
                    error_message: None,
                    timestamp: 0,
                },
            });
        });

        stream
    }
}

fn build_partial_message(
    accumulated_text: &str,
    accumulated_reasoning: &str,
    partial_tool_calls: &HashMap<usize, PartialToolCall>,
    model_id: &str,
) -> Message {
    let mut content_blocks: Vec<ContentBlock> = Vec::new();
    if !accumulated_reasoning.is_empty() {
        content_blocks.push(ContentBlock::Thinking {
            thinking: accumulated_reasoning.to_string(),
            thinking_signature: None,
            redacted: None,
        });
    }
    if !accumulated_text.is_empty() {
        content_blocks.push(ContentBlock::Text {
            text: accumulated_text.to_string(),
            text_signature: None,
        });
    }
    let mut indices: Vec<_> = partial_tool_calls.keys().copied().collect();
    indices.sort();
    for idx in indices {
        if let Some(partial) = partial_tool_calls.get(&idx) {
            let args = if partial.arguments.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_str(&partial.arguments).unwrap_or(serde_json::Value::Null)
            };
            content_blocks.push(ContentBlock::ToolCall {
                id: partial.id.clone().unwrap_or_default(),
                name: partial.name.clone().unwrap_or_default(),
                arguments: args,
                thought_signature: None,
            });
        }
    }
    Message::Assistant {
        content: content_blocks,
        api: "openai-compatible".to_string(),
        provider: "openai-compatible".to_string(),
        model: model_id.to_string(),
        response_id: None,
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    }
}
