pub mod message;
pub use message::{Message, Role};

pub mod types;
pub use types::{GenerateOptions, GenerateRequest, GenerateResponse, Usage};

pub mod error;
pub use error::AIError;

pub mod provider;
pub use provider::LLMProvider;

pub mod providers;
pub use providers::mock::MockProvider;

#[cfg(test)]
mod test {
    use crate::{GenerateRequest, LLMProvider, Message, MockProvider};

    #[tokio::test]
    async fn mock_test_generate_response() {
        let provider = MockProvider;

        let req = GenerateRequest::new(
            "Mock Model",
            vec![
                Message::system("You are a helpful assistant"),
                Message::user("Who are you?"),
            ],
        )
        .temperature(0.7)
        .max_tokens(128);

        let resp = provider.generate(req).await.unwrap();

        assert_eq!(resp.model, "Mock Model");
        assert!(resp.content.contains("Who"));
        assert_eq!(resp.finish_reason.as_deref(), Some("stop"));
    }
}
