use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::message::Message;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub total_tokens: u64,
    pub cost: Cost,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Context {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Tool>>,
}

#[derive(Debug, Clone)]
pub struct GenerateResponse {
    pub model: String,
    pub content: String,
    pub usage: Option<Usage>,
    pub finish_reason: Option<String>,
}

/// Model descriptor aligned with pi-mono's `Model<TApi>`.
/// `api` determines which provider code path runs; `provider` disambiguates
/// compat shims when multiple providers share the same API (e.g. OpenRouter
/// vs Groq both use `"openai-completions"`).
#[derive(Debug, Clone)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub api: String,
    pub provider: String,
    pub base_url: String,
    pub reasoning: bool,
    pub input: Vec<String>,
    pub cost: Cost,
    pub context_window: u64,
    pub max_tokens: u64,
    pub headers: Option<HashMap<String, String>>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            api: String::new(),
            provider: String::new(),
            base_url: String::new(),
            reasoning: false,
            input: vec!["text".into()],
            cost: Cost::default(),
            context_window: 0,
            max_tokens: 0,
            headers: None,
        }
    }
}

/// Options passed to `ApiProvider::stream()`.
#[derive(Debug, Clone, Default)]
pub struct StreamOptions {
    pub api_key: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub stop: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct GenerateRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub options: GenerateOptions,
}

#[derive(Debug, Clone, Default)]
pub struct GenerateOptions {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub stop: Option<Vec<String>>,
}

impl GenerateRequest {
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            options: GenerateOptions::default(),
        }
    }

    pub fn temperature(mut self, temperature: f32) -> Self {
        self.options.temperature = Some(temperature);
        self
    }

    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.options.max_tokens = Some(max_tokens);
        self
    }

    pub fn top_p(mut self, top_p: f32) -> Self {
        self.options.top_p = Some(top_p);
        self
    }
}
