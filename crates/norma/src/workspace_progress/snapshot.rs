use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressTotals {
    pub project_count: usize,
    pub area_count: usize,
    pub role_count: usize,
    pub all_known_logical_task_count: usize,
    pub unfinished_task_count: usize,
    pub attention_task_count: usize,
    /// Recent outcomes overlap task phases; never add these to all_known_logical_task_count.
    pub recent_task_outcome_count: usize,
    pub recent_activity_outcome_count: usize,
    pub all_known_task_phases: BTreeMap<TaskPhase, usize>,
    /// Current default overview: all unfinished, known abnormalities, and window outcomes.
    pub default_report_task_count: usize,
    pub default_report_task_phases: BTreeMap<TaskPhase, usize>,
    /// Extra quality flags; never added to the mutually exclusive task phase total.
    pub quality_task_counts: BTreeMap<ProgressQuality, usize>,
    pub all_known_direct_activity_count: usize,
    pub default_report_activity_count: usize,
    pub unfinished_activity_count: usize,
    pub attention_activity_count: usize,
    /// Unproven historical identities are separate from roles, tracked tasks, and activities.
    pub unresolved_historical_record_count: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressResultTarget {
    pub card: ProgressCardRef,
    pub task_ref: Option<String>,
    pub execution: Option<ProgressExecutionRef>,
    pub result_ref: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskProgressSummary {
    pub card: ProgressCardRef,
    pub logical_task_ref: String,
    pub task_refs: Vec<String>,
    /// Known old-attempt abnormalities remain until a separate product resolution exists.
    pub attention_history: Vec<ProgressAttemptAttention>,
    pub attempt_sequence: u64,
    pub name_at_observation: String,
    pub current_role_name: Option<String>,
    pub title: String,
    pub phase: TaskPhase,
    pub evidence: ProgressEvidence,
    pub observations: Vec<ProgressEvidence>,
    pub quality: Vec<ProgressQuality>,
    pub summary: Option<String>,
    pub waiting_reason: Option<String>,
    pub steps: Option<ProgressSteps>,
    pub needs_review: bool,
    pub terminal_at_unix_ms: Option<u64>,
    pub in_recent_window: bool,
    pub requires_attention: bool,
    pub include_in_default_report: bool,
    pub target: ProgressResultTarget,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressProjectSummary {
    /// False for the explicit unattributed-work bucket; it does not count as a known project.
    pub project_identity_known: bool,
    pub project_id: String,
    pub name: String,
    pub areas: Vec<ProgressArea>,
    pub roles: Vec<ProgressCardFact>,
    pub tasks: Vec<TaskProgressSummary>,
    pub activities: Vec<ActivityProgressSummary>,
    pub recent_outcomes: Vec<ProgressRecentOutcome>,
    /// Always visible for verification; contains no executable card/navigation target.
    pub unresolved_history: Vec<UnresolvedHistoricalRecord>,
    pub totals: ProgressTotals,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceProgressSnapshot {
    pub scope: ProgressScope,
    pub catalog_version: u64,
    pub queried_at_unix_ms: u64,
    pub timezone: String,
    pub recent_window: ProgressWindow,
    pub totals: ProgressTotals,
    pub requested_project_id: Option<String>,
    pub queried_area_ids: Vec<String>,
    pub coverage: Vec<ProgressCoverageFact>,
    /// Completeness only within the explicit scope; a partial mobile grant remains partial.
    pub coverage_complete: bool,
    pub projects: Vec<ProgressProjectSummary>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressOutcomeKind {
    LogicalTask,
    DirectActivity,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressRecentOutcome {
    pub kind: ProgressOutcomeKind,
    pub logical_task_ref: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub phase: TaskPhase,
    pub terminal_at_unix_ms: u64,
    pub evidence: ProgressEvidence,
    pub needs_review: bool,
    pub target: ProgressResultTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActivityProgressSummary {
    pub card: ProgressCardRef,
    pub name_at_observation: String,
    pub current_role_name: Option<String>,
    pub execution: ProgressExecutionRef,
    pub phase: TaskPhase,
    pub evidence: ProgressEvidence,
    pub observations: Vec<ProgressEvidence>,
    pub quality: Vec<ProgressQuality>,
    pub title: String,
    pub summary: Option<String>,
    pub waiting_reason: Option<String>,
    pub steps: Option<ProgressSteps>,
    pub needs_review: bool,
    pub terminal_at_unix_ms: Option<u64>,
    pub in_recent_window: bool,
    pub requires_attention: bool,
    pub include_in_default_report: bool,
    pub target: ProgressResultTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressAttemptAttention {
    pub attempt_sequence: u64,
    pub task_refs: Vec<String>,
    pub phase: TaskPhase,
    pub evidence: ProgressEvidence,
    pub quality: Vec<ProgressQuality>,
    pub summary: Option<String>,
    pub waiting_reason: Option<String>,
    pub needs_review: bool,
    pub target: ProgressResultTarget,
}
