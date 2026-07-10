use crate::{ChatRequest, ChatResponse, LlmError};

pub trait LlmClient: Send {
    fn chat(&mut self, request: &ChatRequest) -> Result<ChatResponse, LlmError>;
}
