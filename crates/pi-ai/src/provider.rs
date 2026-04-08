use async_trait::async_trait;

use crate::{
    error::AIError,
    types::{GenerateRequest, GenerateResponse},
};

#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn generate(&self, req: GenerateRequest) -> Result<GenerateResponse, AIError>;

    fn name(&self) -> &'static str;
}
