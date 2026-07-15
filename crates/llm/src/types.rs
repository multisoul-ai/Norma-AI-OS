use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters_json_schema: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatResponse {
    pub assistant_text: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum LlmError {
    #[error("{0}")]
    Message(String),
}
