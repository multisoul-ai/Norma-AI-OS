//! Bounded display projection. The caller supplies authorized facts; no storage or UI dependency.
use super::{ApprovalSnapshot, DesktopChatWorkspace, TaskProgress};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[path = "workspace_activity_refresh.rs"]
mod activity_refresh;
#[path = "workspace_activity_view.rs"]
mod activity_view;
pub use activity_refresh::ActivityRefreshGate;
#[path = "run_overview.rs"]
mod run_overview;
pub use activity_view::{
    WorkspaceActivityAgentEntry, WorkspaceActivityPendingItem, WorkspaceActivityProjectGroup,
    WorkspaceActivityView,
};
pub use run_overview::{
    project_run_overview, RunOverview, RunOverviewAgent, RunOverviewAttention,
    RunOverviewAttentionKind, RunOverviewFile, RunOverviewPresence, RunOverviewProject,
    RunOverviewProjectPresence,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActivityApprovalFact {
    pub area_id: String,
    pub approval: ApprovalSnapshot,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActivityReviewFact {
    pub area_id: String,
    pub node_id: u64,
    pub profile: String,
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub fingerprint: String,
    pub completed: bool,
    pub available: bool,
    pub conflicted: bool,
    pub accepted: bool,
    pub file_count: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkspaceActivityFacts {
    pub workspace: DesktopChatWorkspace,
    pub observed: bool,
    pub approvals: Vec<ActivityApprovalFact>,
    pub reviews: Vec<ActivityReviewFact>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceActivityStatus {
    Approval,
    Failed,
    Review,
    Running,
    Idle,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceActivityTarget {
    pub area_id: String,
    pub node_id: u64,
    pub profile: String,
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceActivityAgent {
    pub node_id: u64,
    pub name: String,
    pub state: String,
    #[serde(default)]
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgress>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceActivityProject {
    #[serde(default)]
    pub desktop_workspace_id: String,
    #[serde(default)]
    pub workspace_name: String,
    pub area_id: String,
    pub project_id: String,
    pub name: String,
    pub status: WorkspaceActivityStatus,
    pub reason: String,
    pub agents: Vec<WorkspaceActivityAgent>,
    pub other_agents: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<WorkspaceActivityTarget>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceActivitySummary {
    pub machine_id: String,
    pub desktop_workspace_id: String,
    pub name: String,
    pub updated_at: u64,
    pub observed: bool,
    pub active: bool,
    pub running_count: usize,
    pub approval_count: usize,
    pub review_count: usize,
    pub failure_count: usize,
    pub hidden_projects: usize,
    pub projects: Vec<WorkspaceActivityProject>,
    pub attention: Vec<WorkspaceActivityAttention>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceActivityAttention {
    pub target: WorkspaceActivityTarget,
    pub kind: String,
    pub project: String,
    pub agent: String,
    pub reason: String,
}
impl WorkspaceActivitySummary {
    pub fn from_facts(machine: &str, facts: WorkspaceActivityFacts, now: u64) -> Self {
        let mut result = Self::from_facts_unbounded(machine, facts, now);
        result.limit_projects();
        result
    }
    pub fn from_workspaces(machine: &str, facts: Vec<WorkspaceActivityFacts>, now: u64) -> Self {
        let mut result = Self::from_facts_unbounded(
            machine,
            WorkspaceActivityFacts {
                workspace: DesktopChatWorkspace {
                    id: String::new(),
                    name: "Mac 工作总览".into(),
                    revision: 0,
                    projects: vec![],
                },
                observed: true,
                approvals: vec![],
                reviews: vec![],
            },
            now,
        );
        let mut seen = HashSet::new();
        for mut facts in facts {
            for project in &mut facts.workspace.projects {
                project.agents.retain_mut(|a| {
                    if a.card.run_id.is_none()
                        || seen.insert((
                            project.project_id.clone(),
                            a.card.node_id,
                            a.card.profile.clone(),
                            a.card.run_id.clone(),
                            a.card.session_id.clone(),
                            a.card.turn_id.clone(),
                        ))
                    {
                        return true;
                    }
                    let pending = facts.reviews.iter().any(|r| {
                        r.area_id == project.area_id
                            && r.node_id == a.card.node_id
                            && r.profile == a.card.profile
                            && r.completed
                            && r.available
                            && !r.conflicted
                            && !r.accepted
                    });
                    if pending {
                        // Keep the scoped delivery without counting the shared live run twice.
                        a.card.state = "delivered".into();
                        a.card.run_id = None;
                    }
                    pending
                });
            }
            let space = Self::from_facts_unbounded(machine, facts, now);
            result.observed &= space.observed;
            result.active |= space.active;
            result.running_count += space.running_count;
            result.approval_count += space.approval_count;
            result.review_count += space.review_count;
            result.failure_count += space.failure_count;
            result.attention.extend(space.attention);
            result.projects.extend(
                space
                    .projects
                    .into_iter()
                    .filter(|p| p.status != WorkspaceActivityStatus::Idle),
            );
        }
        result.projects.sort_by_key(|p| p.status);
        result.limit_projects();
        result
    }
    fn limit_projects(&mut self) {
        self.hidden_projects = self.projects.len().saturating_sub(3);
        self.projects.truncate(3);
    }
    fn from_facts_unbounded(machine: &str, facts: WorkspaceActivityFacts, now: u64) -> Self {
        activity_view::unbounded_projection(machine, facts, now).0
    }
    pub fn same_display(&self, other: &Self) -> bool {
        let mut a = self.clone();
        let mut b = other.clone();
        a.updated_at = 0;
        b.updated_at = 0;
        a == b
    }
    pub fn needs_attention(&self) -> bool {
        self.approval_count + self.review_count + self.failure_count > 0
    }
}
fn excerpt(value: &str, bytes: usize) -> String {
    let clean = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean: String = clean
        .chars()
        .filter(|c| !c.is_control())
        .take(1024)
        .collect();
    if clean.len() <= bytes {
        return clean;
    }
    let mut end = bytes.saturating_sub(3);
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &clean[..end])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityEvent {
    Start,
    Update,
    End,
}
/// Pure lifecycle decision; send reservations and token authorization belong to the caller.
pub fn activity_event(
    current: &WorkspaceActivitySummary,
    previous: Option<&WorkspaceActivitySummary>,
    started: Option<u64>,
    ended: Option<u64>,
    last_sent: u64,
    quiet_seen: bool,
) -> Option<ActivityEvent> {
    if !current.observed {
        return None;
    }
    if ended.is_some() {
        return (quiet_seen && current.active).then_some(ActivityEvent::Start);
    }
    let Some(started) = started else {
        return current.active.then_some(ActivityEvent::Start);
    };
    if !current.active || current.updated_at.saturating_sub(started) >= 8 * 3600 {
        return Some(ActivityEvent::End);
    }
    let critical = previous.is_none_or(|p| {
        p.attention
            .iter()
            .map(|a| (&a.kind, &a.target))
            .collect::<Vec<_>>()
            != current
                .attention
                .iter()
                .map(|a| (&a.kind, &a.target))
                .collect::<Vec<_>>()
    });
    let elapsed = current.updated_at.saturating_sub(last_sent);
    (critical
        || elapsed >= 60
        || (elapsed >= 30 && previous.is_none_or(|p| !p.same_display(current))))
    .then_some(ActivityEvent::Update)
}
