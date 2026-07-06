use serde::{Deserialize, Serialize};
use std::fmt;

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
    error_message: Option<String>,
}

impl Default for RealtimeSessionState {
    fn default() -> Self {
        Self {
            session_id: None,
            phase: RealtimePhase::Off,
            user_speaking: false,
            assistant_speaking: false,
            error_message: None,
        }
    }
}

impl RealtimeSessionState {
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
        self.error_message = Some(message.into());
    }

    fn is_active(&self) -> bool {
        self.session_id.is_some()
            && matches!(
                self.phase,
                RealtimePhase::Connecting | RealtimePhase::Listening
            )
    }
}

#[cfg(test)]
#[path = "../tests/unit/session_tests.rs"]
mod tests;
