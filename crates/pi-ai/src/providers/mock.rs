use crate::{
    message::{ContentBlock, Message, StopReason, UserContent},
    provider::ApiProvider,
    stream::{AssistantMessageEvent, AssistantMessageEventStream},
    types::{Context, Model, StreamOptions, Usage},
};

pub struct MockProvider;

fn extract_text_from_user_content(content: &UserContent) -> String {
    match content {
        UserContent::Plain(text) => text.clone(),
        UserContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

impl ApiProvider for MockProvider {
    fn api(&self) -> &'static str {
        "mock"
    }

    fn stream(&self, model: &Model, context: &Context, _options: StreamOptions) -> AssistantMessageEventStream {
        let last_user_message = context
            .messages
            .iter()
            .rev()
            .find(|message| matches!(message, Message::User { .. }))
            .map(|message| {
                if let Message::User { content, .. } = message {
                    extract_text_from_user_content(content)
                } else {
                    String::new()
                }
            })
            .unwrap_or_else(|| "empty input".to_string());

        let model_id = model.id.clone();
        let response_text = format!("mock response to: {}", last_user_message);

        let (stream, handle) = AssistantMessageEventStream::new();

        tokio::spawn(async move {
            let partial = Message::Assistant {
                content: vec![],
                api: "mock".to_string(),
                provider: "mock".to_string(),
                model: model_id.clone(),
                response_id: None,
                usage: Usage::default(),
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: 0,
            };

            handle.push(AssistantMessageEvent::Start {
                partial: partial.clone(),
            });
            for (i, ch) in response_text.chars().enumerate() {
                let delta_partial = Message::Assistant {
                    content: vec![ContentBlock::Text {
                        text: response_text.chars().take(i + 1).collect(),
                        text_signature: None,
                    }],
                    api: "mock".to_string(),
                    provider: "mock".to_string(),
                    model: model_id.clone(),
                    response_id: None,
                    usage: Usage::default(),
                    stop_reason: StopReason::Stop,
                    error_message: None,
                    timestamp: 0,
                };
                handle.push(AssistantMessageEvent::TextDelta {
                    content_index: 0,
                    delta: ch.to_string(),
                    partial: delta_partial,
                });
            }

            let final_message = Message::Assistant {
                content: vec![ContentBlock::Text {
                    text: response_text,
                    text_signature: None,
                }],
                api: "mock".to_string(),
                provider: "mock".to_string(),
                model: model_id,
                response_id: None,
                usage: Usage::default(),
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: 0,
            };

            handle.push(AssistantMessageEvent::Done {
                reason: StopReason::Stop,
                message: final_message,
            });
        });

        stream
    }
}
