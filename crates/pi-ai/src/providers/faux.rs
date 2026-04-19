use async_trait::async_trait;
use std::{
    sync::{Arc, Mutex},
    vec,
};

use crate::{
    error::AIError,
    message::{ContentBlock, Message, StopReason, UserContent},
    provider::LLMProvider,
    stream::{AssistantMessageEvent, AssistantMessageEventStream},
    types::{GenerateRequest, GenerateResponse, Usage},
};

pub enum FauxResponseStep {
    Static(Message),
}

pub struct FauxProviderState {
    pub call_count: usize,
}

struct Inner {
    responses: Vec<FauxResponseStep>,
    state: FauxProviderState,
}

pub struct FauxProvider {
    inner: Arc<Mutex<Inner>>,
}

pub struct FauxProviderHandle {
    inner: Arc<Mutex<Inner>>,
}

impl FauxProvider {
    pub fn new() -> (Self, FauxProviderHandle) {
        let inner = Arc::new(Mutex::new(Inner {
            responses: vec![],
            state: FauxProviderState { call_count: 0 },
        }));

        (
            FauxProvider {
                inner: Arc::clone(&inner),
            },
            FauxProviderHandle { inner },
        )
    }
}

impl FauxProviderHandle {
    pub fn set_responses(&self, responses: Vec<FauxResponseStep>) {
        let mut guard = self.inner.lock().unwrap();
        guard.responses = responses;
    }

    pub fn append_responses(&self, responses: Vec<FauxResponseStep>) {
        let mut guard = self.inner.lock().unwrap();
        guard.responses.extend(responses);
    }

    pub fn get_pending_response_count(&self) -> usize {
        let guard = self.inner.lock().unwrap();
        let count = guard.responses.len();
        count
    }

    pub fn clear_responses(&self) {
        let mut guard = self.inner.lock().unwrap();
        guard.responses.clear();
    }
}

fn extract_assistant_text(message: &Message) -> String {
    match message {
        Message::Assistant { content, .. } => content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => panic!(
            "FauxProvider only accepts Assistant message, got {:?}",
            message
        ),
    }
}

fn extract_all_text(messages: &[Message]) -> String {
    messages
        .iter()
        .map(|msg| match msg {
            Message::User { content, .. } => match content {
                UserContent::Plain(text) => text.clone(),
                UserContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            },
            Message::Assistant { content, .. } => content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("")
}

fn estimate_tokens(text: &str) -> u64 {
    ((text.len() + 3) / 4) as u64
}

fn split_into_token_deltas(text: &str, token_size: usize) -> Vec<String> {
    text.chars()
        .collect::<Vec<_>>()
        .chunks(token_size)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect()
}

#[async_trait]
impl LLMProvider for FauxProvider {
    async fn generate(&self, req: GenerateRequest) -> Result<GenerateResponse, AIError> {
        let mut guard = self.inner.lock().unwrap();
        guard.state.call_count += 1;

        let faux_step = guard.responses.remove(0);
        drop(guard);

        let message = match faux_step {
            FauxResponseStep::Static(msg) => msg,
        };

        let text = extract_assistant_text(&message);

        let input = estimate_tokens(&extract_all_text(&req.messages));
        let output = estimate_tokens(&text);

        let usage = Usage {
            input: input,
            output: output,
            cache_read: 0,
            cache_write: 0,
            total_tokens: input + output,
            cost: crate::types::Cost::default(),
        };

        Ok(GenerateResponse {
            model: req.model,
            content: text,
            usage: Some(usage),
            finish_reason: Some("stop".to_string()),
        })
    }

    fn stream(&self, req: GenerateRequest) -> AssistantMessageEventStream {
        let inner = Arc::clone(&self.inner);
        let model = req.model.clone();

        let (stream, handle) = AssistantMessageEventStream::new();

        tokio::spawn(async move {
            let mut guard = inner.lock().unwrap();
            guard.state.call_count += 1;

            if guard.responses.is_empty() {
                let error_message = Message::Assistant {
                    content: vec![],
                    api: "faux".to_string(),
                    provider: "faux".to_string(),
                    model: model.clone(),
                    response_id: None,
                    usage: Usage::default(),
                    stop_reason: StopReason::Error,
                    error_message: Some("no responses registered".to_string()),
                    timestamp: 0,
                };

                handle.push(AssistantMessageEvent::Error {
                    reason: StopReason::Error,
                    error: error_message,
                });
                return;
            }

            let faux_step = guard.responses.remove(0);
            drop(guard);

            let message = match faux_step {
                FauxResponseStep::Static(msg) => msg,
            };

            let text = extract_assistant_text(&message);

            let partial = Message::Assistant {
                content: vec![],
                provider: "faux".to_string(),
                api: "faux".to_string(),
                model: model.clone(),
                response_id: None,
                usage: crate::Usage::default(),
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: 0,
            };

            handle.push(AssistantMessageEvent::Start {
                partial: partial.clone(),
            });

            let deltas = split_into_token_deltas(&text, 2);
            let mut accumulated = String::new();
            for delta in &deltas {
                accumulated.push_str(delta);

                let delta_partial = Message::Assistant {
                    content: vec![ContentBlock::Text {
                        text: accumulated.clone(),
                        text_signature: None,
                    }],
                    provider: "faux".to_string(),
                    api: "faux".to_string(),
                    model: model.clone(),
                    response_id: None,
                    usage: Usage::default(),
                    stop_reason: StopReason::Stop,
                    error_message: None,
                    timestamp: 0,
                };
                handle.push(AssistantMessageEvent::TextDelta {
                    content_index: 0,
                    delta: delta.clone(),
                    partial: delta_partial,
                });
            }

            let final_message = Message::Assistant {
                content: vec![ContentBlock::Text {
                    text: text,
                    text_signature: None,
                }],
                provider: "faux".to_string(),
                api: "faux".to_string(),
                model: model,
                response_id: None,
                usage: crate::Usage::default(),
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
        "faux"
    }
}
