use crate::{AgentMessage, ProviderRequest, ProviderResponse, ProviderToolCall};
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum ProviderError {
    #[error("provider error: {0}")]
    Message(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderStreamEvent {
    TextDelta(String),
    ToolCall(ProviderToolCall),
}

pub trait AgentProvider: Send {
    fn complete(&mut self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError>;

    fn stream(
        &mut self,
        request: &ProviderRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent),
    ) -> Result<ProviderResponse, ProviderError> {
        let response = self.complete(request)?;
        if !response.text.is_empty() {
            emit(ProviderStreamEvent::TextDelta(response.text.clone()));
        }
        for tool_call in &response.tool_calls {
            emit(ProviderStreamEvent::ToolCall(tool_call.clone()));
        }
        Ok(response)
    }

    /// Streams a reply until the caller abandons the current dialogue turn.
    fn stream_with_cancellation(
        &mut self,
        request: &ProviderRequest,
        cancelled: Option<&AtomicBool>,
        emit: &mut dyn FnMut(ProviderStreamEvent),
    ) -> Result<ProviderResponse, ProviderError> {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err(ProviderError::Message(
                "dialogue stream cancelled".to_string(),
            ));
        }
        let response = self.stream(request, emit)?;
        if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err(ProviderError::Message(
                "dialogue stream cancelled".to_string(),
            ));
        }
        Ok(response)
    }
}

pub(crate) fn system_message(content: impl Into<String>) -> AgentMessage {
    AgentMessage {
        role: crate::MessageRole::System,
        content: content.into(),
        tool_call_id: None,
        tool_calls: Vec::new(),
    }
}

pub(crate) fn user_message(content: impl Into<String>) -> AgentMessage {
    AgentMessage {
        role: crate::MessageRole::User,
        content: content.into(),
        tool_call_id: None,
        tool_calls: Vec::new(),
    }
}

pub(crate) fn assistant_message(
    content: impl Into<String>,
    tool_calls: &[ProviderToolCall],
) -> AgentMessage {
    AgentMessage {
        role: crate::MessageRole::Assistant,
        content: content.into(),
        tool_call_id: None,
        tool_calls: tool_calls.to_vec(),
    }
}

pub(crate) fn tool_message(call_id: impl Into<String>, content: impl Into<String>) -> AgentMessage {
    AgentMessage {
        role: crate::MessageRole::Tool,
        content: content.into(),
        tool_call_id: Some(call_id.into()),
        tool_calls: Vec::new(),
    }
}
