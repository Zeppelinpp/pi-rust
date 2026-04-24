use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        text_signature: Option<String>,
    },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        thinking_signature: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        redacted: Option<bool>,
    },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "toolCall")]
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        thought_signature: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    Plain(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}

impl StopReason {
    pub fn to_str(&self) -> &'static str {
        match self {
            StopReason::Stop => "stop",
            StopReason::Length => "length",
            StopReason::ToolUse => "toolUse",
            StopReason::Error => "error",
            StopReason::Aborted => "aborted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "user")]
    User {
        content: UserContent,
        timestamp: u64,
    },
    #[serde(rename = "assistant")]
    Assistant {
        content: Vec<ContentBlock>,
        api: String,
        provider: String,
        model: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        response_id: Option<String>,
        usage: crate::types::Usage,
        stop_reason: StopReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
        timestamp: u64,
    },
    #[serde(rename = "toolResult")]
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        content: Vec<ContentBlock>,
        is_error: bool,
        timestamp: u64,
    },
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
}

// #[derive(Debug, Clone, PartialEq, Eq)]
// pub struct Message {
//     pub role: Role,
//     pub content: String,
// }

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: UserContent::Plain(content.into()),
            timestamp: now_ms(),
        }
    }

    pub fn user_blocks(blocks: Vec<ContentBlock>) -> Self {
        Self::User {
            content: UserContent::Blocks(blocks),
            timestamp: now_ms(),
        }
    }

    pub fn text_block(text: impl Into<String>) -> ContentBlock {
        ContentBlock::Text {
            text: text.into(),
            text_signature: None,
        }
    }

    pub fn image_block(data: impl Into<String>, mime_type: impl Into<String>) -> ContentBlock {
        ContentBlock::Image {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }
}
