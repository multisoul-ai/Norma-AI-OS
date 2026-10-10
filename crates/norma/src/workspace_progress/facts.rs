use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct ProgressCardRef {
    pub workspace_id: String,
    pub area_id: String,
    /// Navigation hint only; never the progress deduplication identity.
    pub node_id: u64,
    pub card_instance_id: String,
}
pub(crate) type CardKey = (String, String, String);
impl ProgressCardRef {
    pub(crate) fn key(&self) -> CardKey {
        (
            self.workspace_id.clone(),
            self.area_id.clone(),
            self.card_instance_id.clone(),
        )
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AreaPresence {
    Foreground,
    Background,
    Unloaded,
    Removed,
    Unknown,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressArea {
    pub workspace_id: String,
    pub area_id: String,
    pub project_id: String,
    pub project_name: String,
    pub presence: AreaPresence,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressCardFact {
    pub card: ProgressCardRef,
    pub name: String,
    pub enabled: bool,
    /// Deleted cards do not increase current role counts; historical tasks remain readable.
    pub retained: bool,
    pub observed_at_unix_ms: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct ProgressExecutionRef {
    pub runtime_instance_id: String,
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TaskPhase {
    Accepted,
    WaitingToSubmit,
    Preparing,
    Submitted,
    Running,
    WaitingApproval,
    WaitingInput,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
    Unknown,
}
impl TaskPhase {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    WorkspaceLedger,
    ProjectLedger,
    Runtime,
    ProviderHook,
    TurnOutcome,
    PersistedSnapshot,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// An adapter has checked the exact task/turn identity and the phase transition evidence.
    Transition,
    /// An observation snapshot cannot prove a terminal outcome.
    Snapshot,
    /// A bounded summary alone cannot prove execution or completion.
    DisplayOnly,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FactFreshness {
    Live,
    Persisted,
    NeedsVerification,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProgressQuality {
    Untracked,
    OutcomeUnknown,
    Stale,
    ConflictingEvidence,
    IncompletePlan,
    UnresolvedLink,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct ProgressEvidence {
    pub source: EvidenceSource,
    pub kind: EvidenceKind,
    /// Original evidence observation time, never the query or later disk-write time.
    pub observed_at_unix_ms: u64,
    /// Monotonic within this source and execution, not a cross-source sequence.
    pub source_sequence: u64,
    pub freshness: FactFreshness,
    pub quality: Vec<ProgressQuality>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressSteps {
    pub completed: u64,
    pub total: u64,
    pub complete_plan: bool,
    pub current_step: Option<String>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskFactKind {
    LogicalTask,
    InternalStep,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskProgressFact {
    pub card: ProgressCardRef,
    pub task_ref: String,
    /// Explicit canonical root linkage. Never infer linkage from titles or names.
    pub logical_task_ref: Option<String>,
    /// Explicit retry/continuation attempt order; later attempts may supersede earlier outcomes.
    pub attempt_sequence: u64,
    pub kind: TaskFactKind,
    pub name_at_observation: String,
    pub created_at_unix_ms: u64,
    pub title: String,
    pub phase: TaskPhase,
    pub execution: Option<ProgressExecutionRef>,
    pub evidence: ProgressEvidence,
    pub summary: Option<String>,
    pub waiting_reason: Option<String>,
    pub steps: Option<ProgressSteps>,
    pub needs_review: bool,
    /// Trusted outcome time if known; otherwise Transition's observation time is used.
    pub terminal_at_unix_ms: Option<u64>,
    pub result_ref: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CardActivityFact {
    pub card: ProgressCardRef,
    pub name_at_observation: String,
    pub execution: ProgressExecutionRef,
    /// Both link fields are required to merge a turn into an existing task attempt.
    pub linked_task_ref: Option<String>,
    pub linked_attempt_sequence: Option<u64>,
    pub phase: TaskPhase,
    pub title: String,
    pub summary: Option<String>,
    pub waiting_reason: Option<String>,
    pub evidence: ProgressEvidence,
    pub steps: Option<ProgressSteps>,
    pub needs_review: bool,
    pub terminal_at_unix_ms: Option<u64>,
    pub result_ref: Option<String>,
}
