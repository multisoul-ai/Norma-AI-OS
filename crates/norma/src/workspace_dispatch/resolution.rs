use super::{
    CardExecutionFingerprint, WorkspaceCardRef, WorkspaceDirectoryRole, WorkspaceProjectRef,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceRoutingMode {
    Execute,
    Discussion,
    Query,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceTargetIntent {
    CallName {
        name: String,
        project: Option<WorkspaceProjectRef>,
    },
    CallNameInProject {
        name: String,
        project_name: String,
    },
    Card {
        card: WorkspaceCardRef,
        project: Option<WorkspaceProjectRef>,
    },
    Project {
        project: WorkspaceProjectRef,
    },
    ProjectName {
        name: String,
    },
    Unspecified,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceResolutionRequest {
    pub mode: WorkspaceRoutingMode,
    pub body: String,
    pub targets: Vec<WorkspaceTargetIntent>,
}

/// Naming has finished; all later waits revalidate this identity and never choose another role.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenWorkspaceTarget {
    pub card: WorkspaceCardRef,
    pub project: WorkspaceProjectRef,
    pub name_at_binding: String,
    pub directory_revision: u64,
    pub execution: Option<CardExecutionFingerprint>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceResolutionReason {
    DirectoryIncomplete,
    InvalidDirectory,
    UnknownName,
    RetiredName,
    DisabledTarget,
    UnsupportedSubmission,
    TargetMissing,
    ProjectMissing,
    AmbiguousProject,
    ProjectMismatch,
    MultipleCandidates,
    NoCandidates,
    MissingTarget,
    EmptyTask,
    ExecutionChanged,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "resolution", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceResolution {
    Ready {
        body: String,
        targets: Vec<FrozenWorkspaceTarget>,
    },
    NeedsTarget {
        body: String,
        reason: WorkspaceResolutionReason,
        candidates: Vec<WorkspaceDirectoryRole>,
    },
    NoDispatch {
        mode: WorkspaceRoutingMode,
        body: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceStructuredMention {
    pub start_byte: usize,
    pub end_byte: usize,
    pub card: WorkspaceCardRef,
    pub project: Option<WorkspaceProjectRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceOriginalTargetEvidence {
    pub start_byte: usize,
    pub end_byte: usize,
    pub literal: String,
    pub target: WorkspaceTargetIntent,
}

/// Constructed from the immutable source message, never the model's paraphrase.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalTargetManifest {
    pub original_body: String,
    pub mode: WorkspaceRoutingMode,
    pub complete: bool,
    pub targets: Vec<WorkspaceTargetIntent>,
    pub evidence: Vec<WorkspaceOriginalTargetEvidence>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspacePlanError {
    ModeDoesNotDispatch,
    AmbiguousOriginalTargets,
    UnresolvedOriginalTargets,
    TargetSetMismatch,
    InvalidEvidence,
    SourceBodyMismatch,
}
