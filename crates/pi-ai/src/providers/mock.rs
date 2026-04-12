use async_trait::async_trait;

use crate::{
    error::AIError,
    message::{ContentBlock, Message, UserContent},
    provider::LLMProvider,
    types::{GenerateRequest, GenerateResponse},
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

    fn name(&self) -> &'static str {
        "mock"
    }
}
