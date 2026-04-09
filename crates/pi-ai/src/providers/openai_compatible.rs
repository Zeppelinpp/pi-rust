use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{
    Usage,
    config::OpenAICompatibleConfig,
    error::AIError,
    message::Role,
    provider::LLMProvider,
    types::{GenerateRequest, GenerateResponse},
};

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
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

fn map_role(role: &Role) -> String {
    match role {
        Role::System => "system".to_string(),
        Role::User => "user".to_string(),
        Role::Assistant => "assistant".to_string(),
    }
}

#[async_trait]
impl LLMProvider for OpenAICompatibleProvider {
    async fn generate(&self, req: GenerateRequest) -> Result<GenerateResponse, AIError> {
        // Err(AIError::Unsupported(
        //     "openai-compatible generate not implemented yet".to_string(),
        // ))
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let body = OpenAICompletionRequest {
            model: req.model,
            messages: req
                .messages
                .into_iter()
                .map(|m| OpenAIMessage {
                    role: map_role(&m.role),
                    content: m.content,
                })
                .collect(),
            temperature: req.options.temperature,
            max_tokens: req.options.max_tokens,
            top_p: req.options.top_p,
            frequency_penalty: req.options.frequency_penalty,
            presence_penalty: req.options.presence_penalty,
            stop: req.options.stop,
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
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            }),
            finish_reason: first_choice.finish_reason,
        })
    }

    fn name(&self) -> &'static str {
        "openai-compatible"
    }
}
