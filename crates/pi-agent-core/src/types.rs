use async_trait::async_trait;
use pi_ai::{
    AssistantMessageEvent, ContentBlock, Message, StopReason, StreamOptions, Usage,
    UserContent,
};
use serde_json::Value;
use std::{any::Any, collections::HashSet, fmt};
use tokio::sync::watch;

// CustomAgentMessage

pub trait CustomAgentMessage: Any + Send + Sync {
    fn timestamp(&self) -> u64;
    fn clone_box(&self) -> Box<dyn CustomAgentMessage>;
}

impl Clone for Box<dyn CustomAgentMessage> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl fmt::Debug for Box<dyn CustomAgentMessage> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomAgentMessage")
            .field("timestamp", &self.timestamp())
            .finish()
    }
}

// AgentMessage

#[derive(Debug, Clone)]
pub enum AgentMessage {
    User {
        content: UserContent,
        timestamp: u64,
    },
    Assistant {
        content: Vec<ContentBlock>,
        api: String,
        provider: String,
        model: String,
        response_id: Option<String>,
        usage: Usage,
        stop_reason: StopReason,
        error_message: Option<String>,
        timestamp: u64,
    },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        content: Vec<ContentBlock>,
        is_error: bool,
        timestamp: u64,
    },
    Custom(Box<dyn CustomAgentMessage>),
}

impl AgentMessage {
    pub fn timestamp(&self) -> u64 {
        match self {
            AgentMessage::User { timestamp, .. } => *timestamp,
            AgentMessage::Assistant { timestamp, .. } => *timestamp,
            AgentMessage::ToolResult { timestamp, .. } => *timestamp,
            AgentMessage::Custom(c) => c.timestamp(),
        }
    }

    pub fn into_llm_message(self) -> Option<Message> {
        match self {
            AgentMessage::User { content, timestamp } => Some(Message::User { content, timestamp }),
            AgentMessage::Assistant {
                content,
                api,
                provider,
                model,
                response_id,
                usage,
                stop_reason,
                error_message,
                timestamp,
            } => Some(Message::Assistant {
                content,
                api,
                provider,
                model,
                response_id,
                usage,
                stop_reason,
                error_message,
                timestamp,
            }),
            AgentMessage::ToolResult {
                tool_call_id,
                tool_name,
                content,
                is_error,
                timestamp,
            } => Some(Message::ToolResult {
                tool_call_id,
                tool_name,
                content,
                is_error,
                timestamp,
            }),
            AgentMessage::Custom(_) => None,
        }
    }
}

impl From<Message> for AgentMessage {
    fn from(msg: Message) -> Self {
        match msg {
            Message::User { content, timestamp } => AgentMessage::User { content, timestamp },
            Message::Assistant {
                content,
                api,
                provider,
                model,
                response_id,
                usage,
                stop_reason,
                error_message,
                timestamp,
            } => AgentMessage::Assistant {
                content,
                api,
                provider,
                model,
                response_id,
                usage,
                stop_reason,
                error_message,
                timestamp,
            },
            Message::ToolResult {
                tool_call_id,
                tool_name,
                content,
                is_error,
                timestamp,
            } => AgentMessage::ToolResult {
                tool_call_id,
                tool_name,
                content,
                is_error,
                timestamp,
            },
        }
    }
}

// AgentEvent

#[derive(Debug, Clone)]
pub enum AgentEvent {
    AgentStart,
    AgentEnd {
        messages: Vec<AgentMessage>,
    },
    TurnStart,
    TurnEnd {
        message: AgentMessage,
        tool_results: Vec<AgentMessage>,
    },
    MessageStart {
        message: AgentMessage,
    },
    MessageUpdate {
        message: AgentMessage,
        assistant_message_event: Box<AssistantMessageEvent>,
    },
    MessageEnd {
        message: AgentMessage,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: Value,
    },
    ToolExecutionUpdate {
        tool_call_id: String,
        tool_name: String,
        args: Value,
        partial_result: Value,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: Value,
        is_error: bool,
    },
}

// ThinkingLevel & ToolExecutionMode

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl Default for ThinkingLevel {
    fn default() -> Self {
        ThinkingLevel::Off
    }
}

impl ThinkingLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThinkingLevel::Off => "off",
            ThinkingLevel::Minimal => "minimal",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
            ThinkingLevel::Xhigh => "xhigh",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolExecutionMode {
    #[default]
    Sequential,
    Parallel,
}

// AgentToolResult & Callback

#[derive(Debug, Clone)]
pub struct AgentToolResult<T = Value> {
    pub content: Vec<ContentBlock>,
    pub details: T,
    pub terminate: bool,
}

pub trait AgentToolUpdateCallback<T = Value>: Send + Sync {
    fn on_update(&self, partial_result: &AgentToolResult<T>);
}

// AgentTool

#[async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> &Value;
    fn label(&self) -> &str;
    fn execution_mode(&self) -> ToolExecutionMode;

    async fn execute(
        &self,
        tool_call_id: &str,
        params: Value,
        signal: Option<&watch::Receiver<bool>>,
        on_update: Option<&dyn AgentToolUpdateCallback>,
    ) -> AgentToolResult;

    fn prepare_arguments(&self, args: Value) -> Value {
        args
    }
}

// AgentState

pub struct AgentState {
    pub system_prompt: String,
    pub model: pi_ai::Model,
    pub thinking_level: ThinkingLevel,
    tools: Vec<Box<dyn AgentTool>>,
    messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub streaming_message: Option<AgentMessage>,
    pub pending_tool_calls: HashSet<String>,
    pub error_message: Option<String>,
}

impl AgentState {
    pub fn new(model: pi_ai::Model) -> Self {
        Self {
            system_prompt: String::new(),
            model,
            thinking_level: ThinkingLevel::default(),
            tools: Vec::new(),
            messages: Vec::new(),
            is_streaming: false,
            streaming_message: None,
            pending_tool_calls: HashSet::new(),
            error_message: None,
        }
    }

    pub fn tools(&self) -> &[Box<dyn AgentTool>] {
        &self.tools
    }

    pub fn set_tools(&mut self, tools: Vec<Box<dyn AgentTool>>) {
        self.tools = tools;
    }

    pub fn messages(&self) -> &[AgentMessage] {
        &self.messages
    }

    pub fn set_messages(&mut self, messages: Vec<AgentMessage>) {
        self.messages = messages;
    }
}

// AgentContext

pub struct AgentContext {
    pub system_prompt: String,
    pub messages: Vec<AgentMessage>,
    pub tools: Option<Vec<Box<dyn AgentTool>>>,
}

// Hook contexts & results

pub struct BeforeToolCallContext<'a> {
    pub assistant_message: &'a Message,
    pub tool_call: &'a ContentBlock,
    pub args: Value,
    pub context: &'a AgentContext,
}

pub struct AfterToolCallContext<'a> {
    pub assistant_message: &'a Message,
    pub tool_call: &'a ContentBlock,
    pub args: Value,
    pub result: AgentToolResult<Value>,
    pub is_error: bool,
    pub context: &'a AgentContext,
}

#[derive(Debug, Default)]
pub struct BeforeToolCallResult {
    pub block: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Default)]
pub struct AfterToolCallResult {
    pub content: Option<Vec<ContentBlock>>,
    pub details: Option<Value>,
    pub is_error: Option<bool>,
    pub terminate: Option<bool>,
}

#[async_trait]
pub trait AgentLoopConfig: Send + Sync {
    fn model(&self) -> &pi_ai::Model;

    fn stream_options(&self) -> StreamOptions {
        StreamOptions::default()
    }

    async fn convert_to_llm(&self, messages: &[AgentMessage]) -> Vec<pi_ai::Message>;

    async fn transform_context<'a>(
        &self,
        messages: &'a [AgentMessage],
        signal: Option<&'a watch::Receiver<bool>>,
    ) -> Vec<AgentMessage> {
        messages.to_vec()
    }

    async fn get_api_key(&self, provider: &str) -> Option<String> {
        None
    }

    async fn get_steering_messages(&self) -> Vec<AgentMessage> {
        vec![]
    }

    async fn get_follow_up_messages(&self) -> Vec<AgentMessage> {
        vec![]
    }

    fn tool_execution(&self) -> ToolExecutionMode {
        ToolExecutionMode::Parallel
    }

    async fn before_tool_call<'a>(
        &self,
        context: BeforeToolCallContext<'a>,
        signal: Option<&'a watch::Receiver<bool>>,
    ) -> Option<BeforeToolCallResult> {
        None
    }

    async fn after_tool_call<'a>(
        &self,
        context: AfterToolCallContext<'a>,
        signal: Option<&'a watch::Receiver<bool>>,
    ) -> Option<AfterToolCallResult> {
        None
    }
}

pub trait StreamFn: Send + Sync {
    fn stream(
        &self,
        model: &pi_ai::Model,
        context: &pi_ai::Context,
        options: pi_ai::StreamOptions,
    ) -> pi_ai::AssistantMessageEventStream;
}
