pub mod message;
pub use message::{ContentBlock, Message, Role, StopReason, UserContent};

pub mod types;
pub use types::{GenerateOptions, GenerateRequest, GenerateResponse, Usage};

pub mod error;
pub use error::AIError;

pub mod provider;
pub use provider::LLMProvider;

pub mod providers;
pub use providers::mock::MockProvider;
pub use providers::openai_compatible::OpenAICompatibleProvider;

pub mod config;
pub use config::OpenAICompatibleConfig;

pub mod stream;
pub use stream::{AssistantMessageEvent, AssistantMessageEventStream, EventStreamHandle};

#[cfg(test)]
mod test {
    use crate::{GenerateRequest, LLMProvider, Message, MockProvider};

    #[tokio::test]
    async fn mock_test_generate_response() {
        let provider = MockProvider;

        let req = GenerateRequest::new("Mock Model", vec![Message::user("Who are you?")])
            .temperature(0.7)
            .max_tokens(128);

        let resp = provider.generate(req).await.unwrap();

        assert_eq!(resp.model, "Mock Model");
        assert!(resp.content.contains("Who"));
        assert_eq!(resp.finish_reason.as_deref(), Some("stop"));
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::{
        GenerateRequest, LLMProvider, Message, OpenAICompatibleConfig, OpenAICompatibleProvider,
    };

    #[tokio::test]
    #[ignore]
    async fn openai_compatible_generate_works() {
        let api_key = std::env::var("PI_AI_API_KEY").expect("PI_AI_API_KEY not set");
        let base_url = std::env::var("PI_AI_BASE_URL").expect("PI_AI_BASE_URL not set");

        let provider = OpenAICompatibleProvider::new(OpenAICompatibleConfig { api_key, base_url });

        let req = GenerateRequest::new(
            "qwen-turbo",
            vec![Message::user("Say hello in one short sentence.")],
        )
        .temperature(0.7)
        .max_tokens(64);

        let resp = provider.generate(req).await.unwrap();

        assert!(!resp.content.trim().is_empty());
    }
}
