//! Consumer-neutral context identities. Paths alone never identify a canvas.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod assembly;
pub use assembly::*;
mod update;
pub use update::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextProject {
    Personal,
    Project {
        project_id: String,
        worktree: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextScope {
    pub workspace_id: String,
    pub area_id: String,
    pub project: ContextProject,
    pub task_id: Option<String>,
    pub conversation_id: Option<String>,
}

impl ContextScope {
    pub fn is_valid(&self) -> bool {
        let valid = |s: &str| !s.trim().is_empty() && s.len() <= 4096;
        valid(&self.workspace_id)
            && valid(&self.area_id)
            && self.task_id.as_deref().is_none_or(valid)
            && self.conversation_id.as_deref().is_none_or(valid)
            && match &self.project {
                ContextProject::Personal => true,
                ContextProject::Project {
                    project_id,
                    worktree,
                } => valid(project_id) && valid(worktree),
            }
    }
    /// A narrow reader can inherit area facts, never another task/conversation's facts.
    /// Authentication of the reader is the host's responsibility.
    pub fn readable_from(&self, reader: &Self) -> bool {
        self.is_valid()
            && reader.is_valid()
            && self.workspace_id == reader.workspace_id
            && self.area_id == reader.area_id
            && self.project == reader.project
            && self
                .task_id
                .as_ref()
                .is_none_or(|id| reader.task_id.as_ref() == Some(id))
            && self
                .conversation_id
                .as_ref()
                .is_none_or(|id| reader.conversation_id.as_ref() == Some(id))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextRef {
    pub scope: ContextScope,
    pub event_id: String,
    /// Revision at which the immutable event was committed, not the latest scope revision.
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextOrigin {
    Voice,
    DoubaoVoice,
    GptLiveVoice,
    Dock,
    Brain,
    Host,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextSource {
    pub origin: ContextOrigin,
    pub stream_id: String,
    pub sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConstraintEnforcement {
    Communicated,
    /// A deny-only host check. It never grants permission to perform an operation.
    Enforced {
        operation: ContextOperation,
        target: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextOperation {
    DeleteCard,
    DispatchTask,
    SendTerminalInput,
    ApproveOperation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextPayload {
    UserInput {
        text: String,
    },
    Receipt {
        operation_id: String,
        status: ReceiptStatus,
        summary: String,
    },
    RevokeConstraint {
        key: String,
        supersedes: ContextRef,
    },
    Goal {
        text: String,
        supersedes: Option<ContextRef>,
    },
    Decision {
        status: DecisionStatus,
        reason: String,
    },
    Report {
        text: String,
    },
    Delivery {
        report: ContextRef,
        status: ReportDelivery,
    },
    Constraint {
        key: String,
        text: String,
        enforcement: ConstraintEnforcement,
        supersedes: Option<ContextRef>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    Allowed,
    Denied,
    AwaitingConfirmation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Accepted,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportDelivery {
    Created,
    Queued,
    Sent,
    Acknowledged,
    Playing,
    Completed,
    Skipped,
    Interrupted,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextReport {
    pub reference: ContextRef,
    pub text: String,
    pub delivery: ReportDelivery,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextEvent {
    pub event_id: String,
    pub scope: ContextScope,
    pub source: ContextSource,
    pub created_at_ms: i64,
    pub related: Vec<ContextRef>,
    pub payload: ContextPayload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecordedContextEvent {
    pub reference: ContextRef,
    pub event: ContextEvent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextConstraint {
    pub reference: ContextRef,
    pub text: String,
    pub enforcement: ConstraintEnforcement,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextFact {
    pub reference: ContextRef,
    pub text: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkingState {
    #[serde(default)]
    pub goal: Option<ContextFact>,
    pub constraints: BTreeMap<String, ContextConstraint>,
    #[serde(default)]
    pub reports: BTreeMap<String, ContextReport>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextSnapshot {
    pub scope: ContextScope,
    pub revision: u64,
    pub state: WorkingState,
    pub events: Vec<RecordedContextEvent>,
}
