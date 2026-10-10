//! Entry-independent, read-only progress contracts. Facts are captured by authorized adapters.
//! This module performs no storage, Runtime, PTY, network, or navigation side effects.
//! `default_report_*` is the current overview, while `all_known_*` explicitly includes history.
//! Recent outcomes are an overlapping result view and must never be added to task counts.
//! Facts retain original observation times. Multi-area sampling is not an atomic PTY snapshot.
//! Adapters must revalidate authorization/catalog identity before returning this snapshot.
mod aggregate;
mod coverage;
mod display;
mod facts;
mod history;
mod link;
mod page;
mod reconcile;
mod snapshot;
pub use aggregate::aggregate_workspace_progress;
pub use coverage::*;
pub use facts::*;
pub use history::{
    UnresolvedHistoricalEvidence, UnresolvedHistoricalFact, UnresolvedHistoricalRecord,
};
pub use page::*;
use serde::{Deserialize, Serialize};
pub use snapshot::*;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressScope {
    pub workspace_id: String,
    /// The complete readable area set. `whole_workspace` never overrides this allowlist.
    pub authorized_area_ids: Vec<String>,
    pub authorization_revision: u64,
    pub whole_workspace: bool,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressWindow {
    /// Inclusive UTC milliseconds, resolved from natural language by the entry adapter.
    pub start_unix_ms: u64,
    pub end_unix_ms: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressQuery {
    pub queried_at_unix_ms: u64,
    /// Echoes the Mac timezone used by the adapter to resolve “today” and “this week”.
    pub timezone: String,
    pub recent_window: Option<ProgressWindow>,
    /// Can only narrow the authorized scope.
    pub project_id: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedProgressFacts {
    pub scope: ProgressScope,
    pub catalog_version: u64,
    pub areas: Vec<ProgressArea>,
    pub cards: Vec<ProgressCardFact>,
    pub tasks: Vec<TaskProgressFact>,
    pub activities: Vec<CardActivityFact>,
    /// Legacy evidence with no proven card/task identity; never manufacture card references.
    pub unresolved_history: Vec<UnresolvedHistoricalFact>,
    pub coverage: Vec<ProgressCoverageFact>,
}
impl AuthorizedProgressFacts {
    pub fn empty(scope: ProgressScope, catalog_version: u64) -> Self {
        Self {
            scope,
            catalog_version,
            areas: vec![],
            cards: vec![],
            tasks: vec![],
            activities: vec![],
            unresolved_history: vec![],
            coverage: vec![],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressError {
    InvalidScope,
    InvalidWindow,
    InvalidFacts,
}
