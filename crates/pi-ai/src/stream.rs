use crate::message::{Message, StopReason};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum AssistantMessageEvent {
    Start {
        partial: Message,
    },
    TextStart {
        content_index: usize,
        partial: Message,
    },
    TextDelta {
        content_index: usize,
        delta: String,
        partial: Message,
    },
    TextEnd {
        content_index: usize,
        content: String,
        partial: Message,
    },
    ThinkingStart {
        content_index: usize,
        partial: Message,
    },
    ThinkingDelta {
        content_index: usize,
        delta: String,
        partial: Message,
    },
    ThinkingEnd {
        content_index: usize,
        content: String,
        partial: Message,
    },
    ToolCallStart {
        content_index: usize,
        partial: Message,
    },
    ToolCallDelta {
        content_index: usize,
        delta: String,
        partial: Message,
    },
    ToolCallEnd {
        content_index: usize,
        partial: Message,
    },
    Done {
        reason: StopReason,
        message: Message,
    },
    Error {
        reason: StopReason,
        error: Message,
    },
}

#[derive(Debug)]
pub struct AssistantMessageEventStream {
    rx: mpsc::UnboundedReceiver<AssistantMessageEvent>,
}

#[derive(Debug, Clone)]
pub struct EventStreamHandle {
    tx: mpsc::UnboundedSender<AssistantMessageEvent>,
}

impl AssistantMessageEventStream {
    pub fn new() -> (Self, EventStreamHandle) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { rx }, EventStreamHandle { tx })
    }

    pub async fn next(&mut self) -> Option<AssistantMessageEvent> {
        self.rx.recv().await
    }

    pub async fn result(mut self) -> Option<Message> {
        while let Some(event) = self.rx.recv().await {
            match event {
                AssistantMessageEvent::Done { message, .. } => return Some(message),
                AssistantMessageEvent::Error { error, .. } => return Some(error),
                _ => {}
            }
        }
        None
    }
}

impl EventStreamHandle {
    pub fn push(&self, event: AssistantMessageEvent) {
        let _ = self.tx.send(event);
    }

    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ContentBlock, Message, StopReason};

    #[tokio::test]
    async fn stream_push_and_result() {
        let (mut stream, handle) = AssistantMessageEventStream::new();

        tokio::spawn(async move {
            let partial = Message::user("hello");
            handle.push(AssistantMessageEvent::Start {
                partial: partial.clone(),
            });
            handle.push(AssistantMessageEvent::TextDelta {
                content_index: 0,
                delta: "world".to_string(),
                partial,
            });
            let done_message = Message::Assistant {
                content: vec![ContentBlock::Text {
                    text: "world".to_string(),
                    text_signature: None,
                }],
                api: "test".to_string(),
                provider: "test".to_string(),
                model: "mock_model".to_string(),
                response_id: None,
                usage: crate::types::Usage::default(),
                stop_reason: StopReason::Stop,
                error_message: None,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };
            handle.push(AssistantMessageEvent::Done {
                reason: StopReason::Stop,
                message: done_message,
            });
        });

        let event1 = stream.next().await.unwrap();
        assert!(matches!(event1, AssistantMessageEvent::Start { .. }));

        let event2 = stream.next().await.unwrap();
        assert!(matches!(event2, AssistantMessageEvent::TextDelta { .. }));

        let result = stream.result().await.unwrap();
        assert!(matches!(result, Message::Assistant { .. }));
    }
}
