use async_trait::async_trait;

use crate::{
    error::AIError,
    stream::AssistantMessageEventStream,
    types::{GenerateRequest, GenerateResponse},
};

#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn generate(&self, req: GenerateRequest) -> Result<GenerateResponse, AIError>;

    fn stream(&self, req: GenerateRequest) -> AssistantMessageEventStream;

    fn name(&self) -> &'static str;
}
