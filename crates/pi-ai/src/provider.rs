use async_trait::async_trait;

use crate::{
    error::AIError,
    message::Message,
    stream::{AssistantMessageEvent, AssistantMessageEventStream},
    types::{Context, GenerateRequest, GenerateResponse, Model, StreamOptions},
};

#[async_trait]
pub trait ApiProvider: Send + Sync {
    fn api(&self) -> &'static str;

    fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: StreamOptions,
    ) -> AssistantMessageEventStream;

    /// Default implementation: delegates to `stream()` and collects the final message.
    async fn generate(&self, req: GenerateRequest) -> Result<GenerateResponse, AIError> {
        let model = Model {
            id: req.model.clone(),
            name: req.model.clone(),
            api: self.api().to_string(),
            provider: self.api().to_string(),
            ..Default::default()
        };
        let context = Context {
            system_prompt: None,
            messages: req.messages,
            tools: None,
        };
        let options = StreamOptions {
            temperature: req.options.temperature,
            max_tokens: req.options.max_tokens,
            top_p: req.options.top_p,
            frequency_penalty: req.options.frequency_penalty,
            presence_penalty: req.options.presence_penalty,
            stop: req.options.stop,
            ..Default::default()
        };

        let mut stream = self.stream(&model, &context, options);

        while let Some(event) = stream.next().await {
            match event {
                AssistantMessageEvent::Done { message, .. } => {
                    let (content, usage, stop_reason) = match message {
                        Message::Assistant {
                            content,
                            usage,
                            stop_reason,
                            ..
                        } => {
                            let text = content
                                .iter()
                                .filter_map(|b| match b {
                                    crate::message::ContentBlock::Text { text, .. } => {
                                        Some(text.as_str())
                                    }
                                    _ => None,
                                })
                                .collect::<String>();
                            (text, Some(usage), Some(stop_reason.to_str().to_string()))
                        }
                        _ => (String::new(), None, None),
                    };
                    return Ok(GenerateResponse {
                        model: model.id,
                        content,
                        usage,
                        finish_reason: stop_reason,
                    });
                }
                AssistantMessageEvent::Error { error, .. } => {
                    let err_msg = match &error {
                        Message::Assistant { error_message, .. } => error_message
                            .clone()
                            .unwrap_or_else(|| "unknown error".into()),
                        _ => "unknown error".into(),
                    };
                    return Err(AIError::Provider(err_msg));
                }
                _ => {}
            }
        }

        Err(AIError::Provider(
            "stream ended without done or error".into(),
        ))
    }
}
