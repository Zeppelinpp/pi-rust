use async_trait::async_trait;

use crate::{
    error::AIError,
    message::{ContentBlock, Message, StopReason, UserContent},
    provider::LLMProvider,
    stream::{AssistantMessageEvent, AssistantMessageEventStream},
    types::{GenerateRequest, GenerateResponse, Usage},
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

#[async_trait]
impl LLMProvider for MockProvider {
    async fn generate(&self, req: GenerateRequest) -> Result<GenerateResponse, AIError> {
        let last_user_message = req
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

        Ok(GenerateResponse {
            model: req.model,
            content: format!("mock response to: {}", last_user_message),
            usage: None,
            finish_reason: Some("stop".to_string()),
        })
    }

    fn stream(&self, req: GenerateRequest) -> AssistantMessageEventStream {
        let last_user_message = req
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

        let model = req.model.clone();
        let response_text = format!("mock response to: {}", last_user_message);

        let (stream, handle) = AssistantMessageEventStream::new();

        tokio::spawn(async move {
            let partial = Message::Assistant {
                content: vec![],
                api: "mock".to_string(),
                provider: "mock".to_string(),
                model: model.clone(),
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
                    model: model.clone(),
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
                model,
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

    fn name(&self) -> &'static str {
        "mock"
    }
}
