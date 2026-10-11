use agent::card_instance::{CardInstanceAssignment, CardInstanceReceipt, CardInstanceTurn};
use agent::workspace_operation::WorkspaceOperationSource;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceAssignmentKind {
    Task,
    Retry {
        previous_assignment_id: String,
    },
    Reply {
        parent_assignment_id: String,
        turn: CardInstanceTurn,
    },
}

/// A prepared existing card. Display snapshots survive project/area/card removal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceOperationAssignment {
    pub assignment: CardInstanceAssignment,
    pub kind: WorkspaceAssignmentKind,
    pub task_ref: String,
    pub logical_task_ref: String,
    pub attempt_sequence: u64,
    pub project_id: String,
    pub project_name: String,
    pub area_name: String,
}

#[derive(Clone, Debug)]
pub struct NewWorkspaceOperation {
    pub source: WorkspaceOperationSource,
    /// Hash of the trusted original mode, mentions, force and continuation parameters.
    pub original_request_fingerprint: String,
    pub assignments: Vec<WorkspaceOperationAssignment>,
    pub expected_role_revision: u64,
    pub authorization: WorkspaceOperationAuthorization,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoredWorkspaceAssignment {
    #[serde(flatten)]
    pub prepared: WorkspaceOperationAssignment,
    pub receipt: CardInstanceReceipt,
    pub input_phase: WorkspaceInputPhase,
    pub accepted_at_ms: u64,
    pub observed_at_ms: u64,
}

impl std::ops::Deref for StoredWorkspaceAssignment {
    type Target = WorkspaceOperationAssignment;
    fn deref(&self) -> &Self::Target {
        &self.prepared
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceInputPhase {
    Queued,
    Armed,
    BodyWritten,
    EnterConfirmed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceOperationReceipt {
    pub source: WorkspaceOperationSource,
    pub original_request_fingerprint: String,
    pub request_fingerprint: String,
    pub assignments: Vec<StoredWorkspaceAssignment>,
    pub accepted_at_ms: u64,
    pub cursor: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceOperationAcceptance {
    pub newly_accepted: bool,
    pub receipt: WorkspaceOperationReceipt,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceOperationEvent {
    pub cursor: u64,
    pub workspace_id: String,
    pub origin_key: String,
    pub operation_id: String,
    pub assignment_id: String,
    pub evidence: String,
    pub observed_at_ms: u64,
    pub receipt: CardInstanceReceipt,
}

/// Non-secret authorization evidence supplied by the trusted entry adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceOperationAuthorization {
    Desktop,
    Mobile {
        device_id: String,
        revision: u64,
        authorized_area_ids: Vec<String>,
    },
}
