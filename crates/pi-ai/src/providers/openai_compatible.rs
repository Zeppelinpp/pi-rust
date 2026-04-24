use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{
    config::OpenAICompatibleConfig,
    error::AIError,
    message::{ContentBlock, Message, StopReason, UserContent},
    provider::ApiProvider,
    stream::{AssistantMessageEvent, AssistantMessageEventStream},
    types::{Context, GenerateResponse, Model, StreamOptions, Usage},
};

#[derive(Clone)]
pub struct OpenAICompatibleProvider {
    config: OpenAICompatibleConfig,
    client: Client,
}

impl OpenAICompatibleProvider {
    pub fn new(config: OpenAICompatibleConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    pub async fn generate_direct(
        &self,
        model: &str,
        messages: Vec<Message>,
        options: &StreamOptions,
    ) -> Result<GenerateResponse, AIError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let body = OpenAICompletionRequest {
            model: model.to_string(),
            messages: messages.into_iter().filter_map(message_to_openai).collect(),
            temperature: options.temperature,
            max_tokens: options.max_tokens,
            top_p: options.top_p,
            frequency_penalty: options.frequency_penalty,
            presence_penalty: options.presence_penalty,
            stop: options.stop.clone(),
        };

        let resp = self
            .client
            .post(url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|err| AIError::Provider(format!("request failed: {err}")))?;

        let status = resp.status();

        if !status.is_success() {
            let text = resp
                .text()
                .await
                .unwrap_or_else(|_| "failed to read error body".to_string());

            return Err(AIError::Provider(format!(
                "provider returned {}: {}",
                status, text
            )));
        }

        let data: OpenAIChatCompletionResponse = resp
            .json()
            .await
            .map_err(|err| AIError::Provider(format!("invalid response json: {err}")))?;

        let first_choice = data
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AIError::Provider("response choices is empty".to_string()))?;

        Ok(GenerateResponse {
            model: data.model,
            content: first_choice.message.content,
            usage: data.usage.map(|u| Usage {
                input: u.prompt_tokens as u64,
                output: u.completion_tokens as u64,
                cache_read: 0,
                cache_write: 0,
                total_tokens: u.total_tokens as u64,
                cost: crate::types::Cost::default(),
            }),
            finish_reason: first_choice.finish_reason,
        })
    }
}

#[derive(Debug, Serialize)]
struct OpenAICompletionRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    top_p: Option<f32>,
    frequency_penalty: Option<f32>,
    presence_penalty: Option<f32>,
    stop: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIChatCompletionResponse {
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIAssistantMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIAssistantMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

fn message_to_openai(m: Message) -> Option<OpenAIMessage> {
    match m {
        Message::User { content, .. } => {
            let text = match content {
                UserContent::Plain(s) => s,
                UserContent::Blocks(blocks) => blocks
                    .into_iter()
                    .map(|b| match b {
                        ContentBlock::Text { text, .. } => text,
                        _ => String::new(),
                    })
                    .collect(),
            };
            Some(OpenAIMessage {
                role: "user".to_string(),
                content: text,
            })
        }
        Message::Assistant { content, .. } => {
            let text = content
                .into_iter()
                .map(|b| match b {
                    ContentBlock::Text { text, .. } => text,
                    _ => String::new(),
                })
                .collect::<String>();
            Some(OpenAIMessage {
                role: "assistant".to_string(),
                content: text,
            })
        }
        Message::ToolResult { .. } => None,
    }
}

impl ApiProvider for OpenAICompatibleProvider {
    fn api(&self) -> &'static str {
        "openai-completions"
    }

    fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: StreamOptions,
    ) -> AssistantMessageEventStream {
        let (stream, handle) = AssistantMessageEventStream::new();
        let provider = self.clone();
        let model_id = model.id.clone();
        let messages = context.messages.clone();

        tokio::spawn(async move {
            match provider
                .generate_direct(&model_id, messages, &options)
                .await
            {
                Ok(resp) => {
                    let usage = resp.usage.clone().unwrap_or_default();
                    let partial = Message::Assistant {
                        content: vec![],
                        api: "openai-compatible".to_string(),
                        provider: "openai-compatible".to_string(),
                        model: resp.model.clone(),
                        response_id: None,
                        usage: usage.clone(),
                        stop_reason: StopReason::Stop,
                        error_message: None,
                        timestamp: 0,
                    };

                    handle.push(AssistantMessageEvent::Start {
                        partial: partial.clone(),
                    });

                    let delta_partial = Message::Assistant {
                        content: vec![ContentBlock::Text {
                            text: resp.content.clone(),
                            text_signature: None,
                        }],
                        api: "openai-compatible".to_string(),
                        provider: "openai-compatible".to_string(),
                        model: resp.model.clone(),
                        response_id: None,
                        usage: usage.clone(),
                        stop_reason: StopReason::Stop,
                        error_message: None,
                        timestamp: 0,
                    };

                    handle.push(AssistantMessageEvent::TextDelta {
                        content_index: 0,
                        delta: resp.content.clone(),
                        partial: delta_partial,
                    });

                    let final_message = Message::Assistant {
                        content: vec![ContentBlock::Text {
                            text: resp.content,
                            text_signature: None,
                        }],
                        api: "openai-compatible".to_string(),
                        provider: "openai-compatible".to_string(),
                        model: resp.model,
                        response_id: None,
                        usage,
                        stop_reason: StopReason::Stop,
                        error_message: None,
                        timestamp: 0,
                    };

                    handle.push(AssistantMessageEvent::Done {
                        reason: StopReason::Stop,
                        message: final_message,
                    });
                }
                Err(err) => {
                    let error_message = Message::Assistant {
                        content: vec![],
                        api: "openai-compatible".to_string(),
                        provider: "openai-compatible".to_string(),
                        model: model_id,
                        response_id: None,
                        usage: Usage::default(),
                        stop_reason: StopReason::Error,
                        error_message: Some(err.to_string()),
                        timestamp: 0,
                    };

                    handle.push(AssistantMessageEvent::Error {
                        reason: StopReason::Error,
                        error: error_message,
                    });
                }
            }
        });

        stream
    }
}
