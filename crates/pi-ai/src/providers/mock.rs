use async_trait::async_trait;

use crate::{
    error::AIError,
    message::Role,
    provider::LLMProvider,
    types::{GenerateRequest, GenerateResponse},
};

pub struct MockProvider;

#[async_trait]
impl LLMProvider for MockProvider {
    async fn generate(&self, req: GenerateRequest) -> Result<GenerateResponse, AIError> {
        let last_user_message = req
            .messages
            .iter()
            .rev()
            .find(|message| matches!(message.role, Role::User))
            .map(|message| message.content.clone())
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
