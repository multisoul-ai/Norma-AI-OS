use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressDetailPage<T> {
    pub items: Vec<T>,
    pub total_items: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
    /// The complete matching scope totals, computed before any pagination.
    pub totals: ProgressTotals,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProgressProjectHeader {
    pub project_identity_known: bool,
    pub project_id: String,
    pub name: String,
    pub totals: ProgressTotals,
}
impl WorkspaceProgressSnapshot {
    /// Project headers do not embed unbounded role/task details; request those pages separately.
    pub fn project_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> ProgressDetailPage<ProgressProjectHeader> {
        let rows: Vec<_> = self
            .projects
            .iter()
            .map(|p| ProgressProjectHeader {
                project_identity_known: p.project_identity_known,
                project_id: p.project_id.clone(),
                name: p.name.clone(),
                totals: p.totals.clone(),
            })
            .collect();
        page(&rows, &self.totals, offset, limit)
    }
}
impl ProgressProjectSummary {
    /// The default current view, never limited by task creation time.
    pub fn task_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> ProgressDetailPage<TaskProgressSummary> {
        let rows: Vec<_> = self
            .tasks
            .iter()
            .filter(|t| t.include_in_default_report)
            .cloned()
            .collect();
        page(&rows, &self.totals, offset, limit)
    }
    pub fn all_known_task_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> ProgressDetailPage<TaskProgressSummary> {
        page(&self.tasks, &self.totals, offset, limit)
    }
    pub fn role_page(&self, offset: usize, limit: usize) -> ProgressDetailPage<ProgressCardFact> {
        page(&self.roles, &self.totals, offset, limit)
    }
    pub fn activity_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> ProgressDetailPage<ActivityProgressSummary> {
        let rows: Vec<_> = self
            .activities
            .iter()
            .filter(|a| a.include_in_default_report)
            .cloned()
            .collect();
        page(&rows, &self.totals, offset, limit)
    }
    pub fn outcome_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> ProgressDetailPage<ProgressRecentOutcome> {
        page(&self.recent_outcomes, &self.totals, offset, limit)
    }
}
fn page<T: Clone>(
    rows: &[T],
    totals: &ProgressTotals,
    offset: usize,
    limit: usize,
) -> ProgressDetailPage<T> {
    let offset = offset.min(rows.len());
    let end = offset.saturating_add(limit.clamp(1, 100)).min(rows.len());
    ProgressDetailPage {
        items: rows[offset..end].to_vec(),
        total_items: rows.len(),
        offset,
        next_offset: (end < rows.len()).then_some(end),
        totals: totals.clone(),
    }
}
