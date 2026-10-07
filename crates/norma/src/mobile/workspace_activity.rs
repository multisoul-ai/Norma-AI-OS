//! Bounded display projection. The caller supplies authorized facts; no storage or UI dependency.
use super::{ApprovalSnapshot, DesktopChatWorkspace, TaskProgress};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
        let workspace_name = excerpt(&facts.workspace.name, 60);
        let mut result = Self {
            machine_id: machine.into(),
            desktop_workspace_id: facts.workspace.id,
            name: excerpt(&facts.workspace.name, 60),
            updated_at: now,
            observed: facts.observed,
            active: false,
            running_count: 0,
            approval_count: 0,
            review_count: 0,
            failure_count: 0,
            hidden_projects: 0,
            projects: Vec::new(),
            attention: Vec::new(),
        };
        let mut seen_approvals = HashSet::new();
        let mut seen_reviews = HashSet::new();
        for project in facts.workspace.projects {
            let mut row = WorkspaceActivityProject {
                desktop_workspace_id: result.desktop_workspace_id.clone(),
                workspace_name: workspace_name.clone(),
                area_id: project.area_id.clone(),
                project_id: project.project_id,
                name: excerpt(&project.name, 48),
                status: WorkspaceActivityStatus::Idle,
                reason: "暂无执行中的任务".into(),
                agents: Vec::new(),
                other_agents: 0,
                target: None,
            };
            let mut agents = Vec::new();
            for agent in &project.agents {
                let card = &agent.card;
                let working = matches!(
                    card.state.as_str(),
                    "thinking" | "running_tool" | "preparing" | "interrupting" | "collecting"
                );
                result.running_count += usize::from(working);
                result.active |= working || card.state == "waiting_approval";
                let progress = card
                    .task_progress
                    .clone()
                    .filter(|p| p.total > 0 && p.completed <= p.total)
                    .map(|mut p| {
                        p.current_step = p.current_step.map(|s| excerpt(&s, 60));
                        p
                    });
                let mut priority = WorkspaceActivityStatus::Idle;
                let mut reason = None;
                let mut target = WorkspaceActivityTarget {
                    area_id: project.area_id.clone(),
                    node_id: card.node_id,
                    profile: card.profile.clone(),
                    run_id: card.run_id.clone().unwrap_or_default(),
                    session_id: card.session_id.clone().unwrap_or_default(),
                    turn_id: card.turn_id.clone().unwrap_or_default(),
                    request_id: None,
                    fingerprint: None,
                };
                for item in facts
                    .approvals
                    .iter()
                    .filter(|a| a.area_id == project.area_id)
                {
                    let t = &item.approval.target;
                    if t.valid()
                        && !item.approval.decisions.is_empty()
                        && t.node_id == card.node_id
                        && card.profile == t.profile
                        && card.run_id.as_ref() == Some(&t.run_id)
                        && card.session_id.as_ref() == Some(&t.session_id)
                        && card.turn_id.as_ref() == Some(&t.turn_id)
                        && seen_approvals.insert((
                            project.area_id.clone(),
                            t.node_id,
                            t.profile.clone(),
                            t.run_id.clone(),
                            t.session_id.clone(),
                            t.turn_id.clone(),
                            t.request_id.clone(),
                        ))
                    {
                        result.approval_count += 1;
                        priority = WorkspaceActivityStatus::Approval;
                        reason = Some(format!(
                            "{} · 等待你确认 {}",
                            excerpt(&agent.name, 30),
                            excerpt(&item.approval.tool, 32)
                        ));
                        target.request_id = Some(t.request_id.clone());
                        result.attention.push(WorkspaceActivityAttention {
                            target: target.clone(),
                            kind: "approval".into(),
                            project: row.name.clone(),
                            agent: excerpt(&agent.name, 30),
                            reason: reason.clone().unwrap_or_default(),
                        });
                    }
                }
                if card.state == "failed" {
                    result.failure_count += 1;
                    if !target.run_id.is_empty() && !target.turn_id.is_empty() {
                        let mut failed = target.clone();
                        failed.request_id = None;
                        result.attention.push(WorkspaceActivityAttention {
                            target: failed,
                            kind: "failed".into(),
                            project: row.name.clone(),
                            agent: excerpt(&agent.name, 30),
                            reason: excerpt(
                                card.task_summary.as_deref().unwrap_or("执行失败，查看原因"),
                                80,
                            ),
                        });
                    }
                    if priority > WorkspaceActivityStatus::Failed {
                        priority = WorkspaceActivityStatus::Failed;
                        reason = Some(format!(
                            "{} · {}",
                            excerpt(&agent.name, 30),
                            excerpt(
                                card.task_summary.as_deref().unwrap_or("执行失败，查看原因"),
                                80
                            )
                        ));
                    }
                }
                for review in facts.reviews.iter().filter(|r| {
                    r.area_id == project.area_id
                        && r.node_id == card.node_id
                        && r.profile == card.profile
                }) {
                    if review.completed
                        && review.available
                        && !review.conflicted
                        && !review.accepted
                        && seen_reviews
                            .insert((project.area_id.clone(), review.fingerprint.clone()))
                    {
                        result.review_count += 1;
                        let mut review_target = target.clone();
                        review_target.request_id = None;
                        review_target.run_id = review.run_id.clone();
                        review_target.session_id = review.session_id.clone();
                        review_target.turn_id = review.turn_id.clone();
                        review_target.fingerprint = Some(review.fingerprint.clone());
                        result.attention.push(WorkspaceActivityAttention {
                            target: review_target,
                            kind: "review".into(),
                            project: row.name.clone(),
                            agent: excerpt(&agent.name, 30),
                            reason: format!("{} 个文件待验收", review.file_count),
                        });
                        if priority > WorkspaceActivityStatus::Review {
                            priority = WorkspaceActivityStatus::Review;
                            reason = Some(format!(
                                "{} · {} 个文件待验收",
                                excerpt(&agent.name, 30),
                                review.file_count
                            ));
                            target.run_id = review.run_id.clone();
                            target.session_id = review.session_id.clone();
                            target.turn_id = review.turn_id.clone();
                            target.fingerprint = Some(review.fingerprint.clone());
                        }
                    }
                }
                if working && priority > WorkspaceActivityStatus::Running {
                    priority = WorkspaceActivityStatus::Running;
                    reason = Some(format!(
                        "{} · {}",
                        excerpt(&agent.name, 30),
                        excerpt(
                            progress
                                .as_ref()
                                .and_then(|p| p.current_step.as_deref())
                                .or(card.task_summary.as_deref())
                                .or(card.task_title.as_deref())
                                .unwrap_or("正在执行"),
                            80
                        )
                    ));
                }
                if priority < row.status {
                    row.status = priority;
                    row.reason = reason.unwrap_or_default();
                    row.target = (!target.run_id.is_empty()
                        && !target.session_id.is_empty()
                        && !target.turn_id.is_empty())
                    .then_some(target);
                }
                agents.push((
                    priority,
                    WorkspaceActivityAgent {
                        node_id: card.node_id,
                        name: excerpt(&agent.name, 30),
                        state: card.state.clone(),
                        action: excerpt(
                            &progress
                                .as_ref()
                                .map(|p| {
                                    format!(
                                        "{}/{} · {}",
                                        p.completed,
                                        p.total,
                                        p.current_step.as_deref().unwrap_or("执行中")
                                    )
                                })
                                .unwrap_or_else(|| {
                                    card.task_title
                                        .clone()
                                        .unwrap_or_else(|| card.state.clone())
                                }),
                            60,
                        ),
                        progress,
                    },
                ));
            }
            agents.sort_by_key(|(priority, _)| *priority);
            row.other_agents = agents.len().saturating_sub(2);
            row.agents = agents.into_iter().take(2).map(|(_, a)| a).collect();
            result.projects.push(row);
        }
        result.active |= result.approval_count + result.review_count + result.failure_count > 0;
        result.projects.sort_by_key(|p| p.status);
        result
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
