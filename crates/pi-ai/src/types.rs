use crate::message::Message;

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

#[derive(Debug, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct GenerateResponse {
    pub model: String,
    pub content: String,
    pub usage: Option<Usage>,
    pub finish_reason: Option<String>,
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
