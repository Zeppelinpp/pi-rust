pub mod message;
pub use message::{ContentBlock, Message, Role, StopReason, UserContent};

pub mod types;
pub use types::{GenerateOptions, GenerateRequest, GenerateResponse, Usage};

pub mod error;
pub use error::AIError;

pub mod provider;
pub use provider::LLMProvider;

pub mod providers;
pub use providers::faux::{FauxProvider, FauxResponseStep};
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
        AssistantMessageEvent, ContentBlock, GenerateRequest, LLMProvider, Message, MockProvider,
        OpenAICompatibleConfig, OpenAICompatibleProvider,
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

    #[tokio::test]
    async fn mock_test_stream_response() {
        let provider = MockProvider;
        let req = GenerateRequest::new("MockModel", vec![Message::user("Who are you?")])
            .temperature(0.8)
            .max_tokens(128);

        let mut stream = provider.stream(req);
        let mut deltas = 0;

        while let Some(event) = stream.next().await {
            match event {
                AssistantMessageEvent::Start { .. } => {}
                AssistantMessageEvent::TextDelta { delta, .. } => {
                    assert!(!delta.is_empty());
                    deltas += 1;
                }
                AssistantMessageEvent::Done { message, .. } => {
                    if let Message::Assistant { content, .. } = message {
                        let text = content
                            .iter()
                            .map(|b| match b {
                                ContentBlock::Text { text, .. } => text.as_str(),
                                _ => "",
                            })
                            .collect::<Vec<_>>()
                            .join("");
                        assert!(text.contains("mock response to:"));
                        assert!(text.contains("Who are you?"));
                    } else {
                        panic!("Expected assistant message");
                    }
                    break;
                }
                other => panic!("unexpected event: {:?}", other),
            }
        }

        assert!(deltas > 0, "expected at least one text delta");
    }
}

#[cfg(test)]
mod faux_tests {
    use crate::{
        AssistantMessageEvent, ContentBlock, FauxProvider, FauxResponseStep, GenerateRequest,
        LLMProvider, Message, StopReason, Usage,
    };

    fn assistant_message(text: &str) -> Message {
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

    #[tokio::test]
    async fn faux_generate_returns_registered_response() {
        let (provider, handle) = FauxProvider::new();
        handle.set_responses(vec![FauxResponseStep::Static(assistant_message(
            "hello from faux",
        ))]);

        let req = GenerateRequest::new("faux-model", vec![Message::user("hi")]);
        let resp = provider.generate(req).await.unwrap();

        assert_eq!(resp.content, "hello from faux");
        assert_eq!(resp.model, "faux-model");
        assert_eq!(resp.finish_reason.as_deref(), Some("stop"));
    }

    #[tokio::test]
    async fn faux_stream_emits_events_in_order() {
        let (provider, handle) = FauxProvider::new();
        handle.set_responses(vec![FauxResponseStep::Static(assistant_message(
            "hello",
        ))]);

        let req = GenerateRequest::new("faux-model", vec![Message::user("hi")]);
        let mut stream = provider.stream(req);

        let mut event_types = Vec::new();
        let mut final_text = String::new();

        while let Some(event) = stream.next().await {
            match event {
                AssistantMessageEvent::Start { .. } => event_types.push("start"),
                AssistantMessageEvent::TextDelta { delta, .. } => {
                    event_types.push("delta");
                    final_text.push_str(&delta);
                }
                AssistantMessageEvent::Done { message, .. } => {
                    event_types.push("done");
                    if let Message::Assistant { content, .. } = message {
                        let text = content
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text, .. } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("");
                        assert_eq!(text, "hello");
                    }
                    break;
                }
                other => panic!("unexpected event: {:?}", other),
            }
        }

        assert_eq!(event_types, vec!["start", "delta", "delta", "delta", "done"]);
        assert_eq!(final_text, "hello");
    }

    #[tokio::test]
    async fn faux_stream_empty_queue_emits_error() {
        let (provider, _handle) = FauxProvider::new();

        let req = GenerateRequest::new("faux-model", vec![Message::user("hi")]);
        let mut stream = provider.stream(req);

        while let Some(event) = stream.next().await {
            match event {
                AssistantMessageEvent::Error { reason, .. } => {
                    assert_eq!(reason, StopReason::Error);
                    return;
                }
                other => panic!("expected error event, got {:?}", other),
            }
        }

        panic!("stream ended without error event");
    }

    #[tokio::test]
    async fn faux_handle_methods_work() {
        let (provider, handle) = FauxProvider::new();

        assert_eq!(handle.get_pending_response_count(), 0);

        handle.set_responses(vec![
            FauxResponseStep::Static(assistant_message("first")),
            FauxResponseStep::Static(assistant_message("second")),
        ]);
        assert_eq!(handle.get_pending_response_count(), 2);

        handle.append_responses(vec![FauxResponseStep::Static(assistant_message(
            "third",
        ))]);
        assert_eq!(handle.get_pending_response_count(), 3);

        let req = GenerateRequest::new("faux-model", vec![Message::user("hi")]);
        let _ = provider.generate(req).await.unwrap();
        assert_eq!(handle.get_pending_response_count(), 2);

        handle.clear_responses();
        assert_eq!(handle.get_pending_response_count(), 0);
    }

    #[tokio::test]
    async fn faux_fifo_ordering() {
        let (provider, handle) = FauxProvider::new();
        handle.set_responses(vec![
            FauxResponseStep::Static(assistant_message("first")),
            FauxResponseStep::Static(assistant_message("second")),
        ]);

        let req1 = GenerateRequest::new("faux-model", vec![Message::user("a")]);
        let resp1 = provider.generate(req1).await.unwrap();
        assert_eq!(resp1.content, "first");

        let req2 = GenerateRequest::new("faux-model", vec![Message::user("b")]);
        let resp2 = provider.generate(req2).await.unwrap();
        assert_eq!(resp2.content, "second");
    }
}
