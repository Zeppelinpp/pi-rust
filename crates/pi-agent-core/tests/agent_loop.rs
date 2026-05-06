use async_trait::async_trait;
use pi_agent_core::{
    AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, StreamFn, agent_loop::run_agent_loop,
};
use pi_ai::{
    ApiProvider, AssistantMessageEventStream, ContentBlock, Context as LlmContext, FauxProvider,
    FauxResponseStep, Message, Model, StopReason, StreamOptions, Usage, UserContent,
};

struct ProviderStreamFn<P: ApiProvider>(P);

impl<P: ApiProvider> StreamFn for ProviderStreamFn<P> {
    fn stream(
        &self,
        model: &pi_ai::Model,
        context: &pi_ai::Context,
        options: pi_ai::StreamOptions,
    ) -> pi_ai::AssistantMessageEventStream {
        self.0.stream(model, context, options)
    }
}

struct TestConfig {
    model: Model,
}

#[async_trait]
impl AgentLoopConfig for TestConfig {
    fn model(&self) -> &pi_ai::Model {
        &self.model
    }

    async fn convert_to_llm(&self, messages: &[AgentMessage]) -> Vec<Message> {
        messages
            .iter()
            .filter_map(|m| m.clone().into_llm_message())
            .collect()
    }
}

fn faux_model() -> Model {
    Model {
        id: "faux-model".into(),
        name: "faux-model".into(),
        ..Default::default()
    }
}

fn assistant_text(text: &str) -> Message {
    Message::Assistant {
        content: vec![ContentBlock::Text {
            text: text.into(),
            text_signature: None,
        }],
        api: "faux".into(),
        provider: "faux".into(),
        model: "faux-model".into(),
        response_id: None,
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Tag {
    AgentStart,
    AgentEnd,
    TurnStart,
    TurnEnd,
    MessageStart,
    MessageUpdate,
    MessageEnd,
    ToolStart,
    ToolEnd,
}

fn tag(e: &AgentEvent) -> Tag {
    match e {
        AgentEvent::AgentStart => Tag::AgentStart,
        AgentEvent::AgentEnd { .. } => Tag::AgentEnd,
        AgentEvent::TurnStart => Tag::TurnStart,
        AgentEvent::TurnEnd { .. } => Tag::TurnEnd,
        AgentEvent::MessageStart { .. } => Tag::MessageStart,
        AgentEvent::MessageUpdate { .. } => Tag::MessageUpdate,
        AgentEvent::MessageEnd { .. } => Tag::MessageEnd,
        AgentEvent::ToolExecutionStart { .. } => Tag::ToolStart,
        AgentEvent::ToolExecutionEnd { .. } => Tag::ToolEnd,
        AgentEvent::ToolExecutionUpdate { .. } => Tag::ToolEnd, // 暂不区分
    }
}

#[tokio::test]
async fn pure_text_response() {
    let (provider, handle) = FauxProvider::new();
    handle.set_responses(vec![FauxResponseStep::Static(assistant_text(
        "hello world",
    ))]);

    let stream_fn = ProviderStreamFn(provider);
    let config = TestConfig {
        model: faux_model(),
    };
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        tools: None,
    };

    let mut events: Vec<AgentEvent> = Vec::new();
    let mut emit = |e| events.push(e);

    let prompt = AgentMessage::User {
        content: UserContent::Plain("Hi".into()),
        timestamp: 0,
    };

    let new_messages = run_agent_loop(
        vec![prompt],
        &mut context,
        &config,
        None,
        &mut emit,
        &stream_fn,
    )
    .await;

    let tags: Vec<Tag> = events.iter().map(tag).collect();
    assert!(matches!(
        tags.as_slice(),
        [
                Tag::AgentStart,
                Tag::TurnStart,
                Tag::MessageStart,
                Tag::MessageEnd,
                Tag::MessageStart,
                updates @ ..,
                Tag::MessageEnd,
                Tag::TurnEnd,
                Tag::AgentEnd,
        ] if !updates.is_empty()
            && updates.iter().all(|t| *t == Tag::MessageUpdate)
    ));
}

#[tokio::test]
async fn error_stop_reason() {
    let (provider, _handle) = FauxProvider::new();

    let stream_fn = ProviderStreamFn(provider);
    let config = TestConfig {
        model: faux_model(),
    };
    let mut context = AgentContext {
        system_prompt: String::new(),
        messages: vec![],
        tools: None,
    };

    let mut events: Vec<AgentEvent> = Vec::new();
    let mut emit = |e| events.push(e);

    let prompt = AgentMessage::User {
        content: UserContent::Plain("Hi".into()),
        timestamp: 0,
    };

    run_agent_loop(
        vec![prompt],
        &mut context,
        &config,
        None,
        &mut emit,
        &stream_fn,
    )
    .await;

    let last = context.messages.last().expect("no messages");
    if let AgentMessage::Assistant {
        stop_reason,
        error_message,
        ..
    } = last
    {
        assert_eq!(*stop_reason, StopReason::Error);
        assert!(error_message.is_some());
    } else {
        panic!("expected assistant, got {:?}", last);
    }
}
