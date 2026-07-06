use crate::protocol::RealtimeToolResult;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;

const DEFAULT_CALLBACK_QUEUE_LIMIT: usize = 16;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RealtimeSessionId(String);

impl RealtimeSessionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RealtimeSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for RealtimeSessionId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for RealtimeSessionId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RealtimeSessionConfig {
    pub session_id: RealtimeSessionId,
    pub model: String,
    pub tools: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RealtimeStatus {
    Off,
    Connecting,
    Listening,
    UserSpeaking,
    AssistantSpeaking,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RealtimeSessionError {
    #[error("realtime session already active: {existing_session_id}")]
    AlreadyActive {
        existing_session_id: RealtimeSessionId,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RealtimeCallbackUpdate {
    pub injected: Vec<RealtimeToolResult>,
    pub dropped: Vec<RealtimeToolResult>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RealtimePhase {
    Off,
    Connecting,
    Listening,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealtimeSessionState {
    session_id: Option<RealtimeSessionId>,
    phase: RealtimePhase,
    user_speaking: bool,
    assistant_speaking: bool,
    active_response_id: Option<String>,
    callback_queue_limit: usize,
    pending_tool_results: VecDeque<RealtimeToolResult>,
    error_message: Option<String>,
}

impl Default for RealtimeSessionState {
    fn default() -> Self {
        Self::new(DEFAULT_CALLBACK_QUEUE_LIMIT)
    }
}

impl RealtimeSessionState {
    fn new(callback_queue_limit: usize) -> Self {
        Self {
            session_id: None,
            phase: RealtimePhase::Off,
            user_speaking: false,
            assistant_speaking: false,
            active_response_id: None,
            callback_queue_limit: callback_queue_limit.max(1),
            pending_tool_results: VecDeque::new(),
            error_message: None,
        }
    }

    pub fn with_callback_queue_limit(callback_queue_limit: usize) -> Self {
        Self::new(callback_queue_limit)
    }

    pub fn status(&self) -> RealtimeStatus {
        match self.phase {
            RealtimePhase::Off => RealtimeStatus::Off,
            RealtimePhase::Connecting => RealtimeStatus::Connecting,
            RealtimePhase::Listening => {
                if self.assistant_speaking {
                    RealtimeStatus::AssistantSpeaking
                } else if self.user_speaking {
                    RealtimeStatus::UserSpeaking
                } else {
                    RealtimeStatus::Listening
                }
            }
            RealtimePhase::Error => RealtimeStatus::Error,
        }
    }

    pub fn session_id(&self) -> Option<&RealtimeSessionId> {
        self.session_id.as_ref()
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    pub fn start_connecting(
        &mut self,
        session_id: RealtimeSessionId,
    ) -> Result<(), RealtimeSessionError> {
        if self.is_active() {
            let existing_session_id = self
                .session_id
                .clone()
                .expect("active realtime session should have a session id");
            return Err(RealtimeSessionError::AlreadyActive {
                existing_session_id,
            });
        }

        self.session_id = Some(session_id);
        self.phase = RealtimePhase::Connecting;
        self.user_speaking = false;
        self.assistant_speaking = false;
        self.active_response_id = None;
        self.pending_tool_results.clear();
        self.error_message = None;
        Ok(())
    }

    pub fn mark_started(&mut self) {
        if self.session_id.is_some() {
            self.phase = RealtimePhase::Listening;
            self.error_message = None;
        }
    }

    pub fn mark_user_speaking(&mut self, speaking: bool) {
        self.user_speaking = speaking;
    }

    pub fn mark_assistant_speaking(&mut self, speaking: bool) {
        self.assistant_speaking = speaking;
    }

    pub fn mark_error(&mut self, message: impl Into<String>) {
        self.phase = RealtimePhase::Error;
        self.user_speaking = false;
        self.assistant_speaking = false;
        self.active_response_id = None;
        self.pending_tool_results.clear();
        self.error_message = Some(message.into());
    }

    pub fn queue_or_inject_tool_result(
        &mut self,
        result: RealtimeToolResult,
    ) -> RealtimeCallbackUpdate {
        let mut update = RealtimeCallbackUpdate::default();
        if self.can_inject_tool_result_now() {
            update.injected.push(result);
            return update;
        }

        if self.pending_tool_results.len() >= self.callback_queue_limit {
            if let Some(dropped) = self.pending_tool_results.pop_front() {
                update.dropped.push(dropped);
            }
        }
        self.pending_tool_results.push_back(result);
        update
    }

    pub fn handle_transcript_final(&mut self) -> RealtimeCallbackUpdate {
        self.user_speaking = false;
        self.flush_pending_tool_results_if_idle()
    }

    pub fn handle_assistant_audio_end(&mut self) -> RealtimeCallbackUpdate {
        self.assistant_speaking = false;
        self.flush_pending_tool_results_if_idle()
    }

    pub fn pending_tool_result_count(&self) -> usize {
        self.pending_tool_results.len()
    }

    fn is_active(&self) -> bool {
        self.session_id.is_some()
            && matches!(
                self.phase,
                RealtimePhase::Connecting | RealtimePhase::Listening
            )
    }

    fn can_inject_tool_result_now(&self) -> bool {
        matches!(self.phase, RealtimePhase::Listening)
            && !self.user_speaking
            && !self.assistant_speaking
    }

    fn flush_pending_tool_results_if_idle(&mut self) -> RealtimeCallbackUpdate {
        if !self.can_inject_tool_result_now() {
            return RealtimeCallbackUpdate::default();
        }

        RealtimeCallbackUpdate {
            injected: self.pending_tool_results.drain(..).collect(),
            dropped: Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/session_tests.rs"]
mod tests;
