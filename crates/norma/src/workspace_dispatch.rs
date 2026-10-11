//! Entry-independent card addresses. Names and definition IDs are never routing identity.
use serde::{Deserialize, Serialize};
use std::fmt;
mod directory;
mod resolution;
pub use directory::*;
pub use resolution::*;

/// Durable address; a restored card keeps this reference after its Runtime is replaced.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceCardRef {
    pub workspace_id: String,
    pub area_id: String,
    pub node_id: u64,
    pub card_instance_id: String,
}

impl WorkspaceCardRef {
    pub fn validate(&self) -> Result<(), WorkspaceTargetError> {
        if self.node_id == 0
            || [&self.workspace_id, &self.area_id, &self.card_instance_id]
                .into_iter()
                .any(|id| !valid_id(id))
        {
            return Err(WorkspaceTargetError::InvalidIdentity);
        }
        Ok(())
    }
}

/// Volatile identity captured immediately before an asynchronous execution boundary.
/// Missing Provider IDs mean they are not known; they never act as wildcards.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardExecutionFingerprint {
    pub runtime_instance_id: String,
    pub route_generation: u64,
    pub terminal_session_id: Option<u64>,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
}

impl CardExecutionFingerprint {
    pub fn validate(&self) -> Result<(), WorkspaceTargetError> {
        if !valid_id(&self.runtime_instance_id)
            || self.terminal_session_id == Some(0)
            || [self.run_id.as_deref(), self.session_id.as_deref()]
                .into_iter()
                .flatten()
                .any(|id| id.is_empty() || id.len() > 512 || id.chars().any(char::is_control))
        {
            return Err(WorkspaceTargetError::InvalidIdentity);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceCardTarget {
    pub card: WorkspaceCardRef,
    pub execution: CardExecutionFingerprint,
}

impl WorkspaceCardTarget {
    pub fn validate(&self) -> Result<(), WorkspaceTargetError> {
        self.card.validate()?;
        self.execution.validate()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceTargetError {
    InvalidIdentity,
    TargetMissing,
    CardChanged,
    RouteChanged,
    RuntimeChanged,
    RunChanged,
    SessionChanged,
}

impl WorkspaceTargetError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidIdentity => "invalid_identity",
            Self::TargetMissing => "target_missing",
            Self::CardChanged => "card_changed",
            Self::RouteChanged => "route_changed",
            Self::RuntimeChanged => "runtime_changed",
            Self::RunChanged => "run_changed",
            Self::SessionChanged => "session_changed",
        }
    }
}

impl fmt::Display for WorkspaceTargetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for WorkspaceTargetError {}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
