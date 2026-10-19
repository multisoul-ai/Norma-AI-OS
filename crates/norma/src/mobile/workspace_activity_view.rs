use std::collections::HashSet;

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceActivityAgentEntry {
    pub area_id: String,
    pub node_id: u64,
    pub name: String,
    pub raw_state: String,
    pub status: super::WorkspaceActivityStatus,
    pub task_title: Option<String>,
    pub started_at_unix_ms: Option<u64>,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceActivityPendingItem {
    pub kind: String,
    pub area_id: String,
    pub node_id: u64,
    pub project: String,
    pub agent: String,
    pub reason: String,
    pub request_id: Option<String>,
    pub fingerprint: Option<String>,
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceActivityProjectGroup {
    pub area_id: String,
    pub project_id: String,
    pub name: String,
    pub status: super::WorkspaceActivityStatus,
    pub agents: Vec<WorkspaceActivityAgentEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceActivityView {
    pub agents: Vec<WorkspaceActivityAgentEntry>,
    pub pending: Vec<WorkspaceActivityPendingItem>,
    pub groups: Vec<WorkspaceActivityProjectGroup>,
}

impl WorkspaceActivityView {
    pub fn from_facts(facts: &super::WorkspaceActivityFacts) -> Self {
        unbounded_projection("", facts.clone(), 0).1
    }
}

pub(super) fn unbounded_projection(
    machine: &str,
    facts: super::WorkspaceActivityFacts,
    now: u64,
) -> (WorkspaceActivitySummary, WorkspaceActivityView) {
    let mut listed = Vec::new();
    let mut pending = Vec::new();
    let mut groups = Vec::new();
    let workspace_name = super::excerpt(&facts.workspace.name, 60);
    let mut result = super::WorkspaceActivitySummary {
        machine_id: machine.into(),
        desktop_workspace_id: facts.workspace.id,
        name: super::excerpt(&facts.workspace.name, 60),
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
            name: super::excerpt(&project.name, 48),
            status: WorkspaceActivityStatus::Idle,
            reason: "暂无执行中的任务".into(),
            agents: Vec::new(),
            other_agents: 0,
            target: None,
        };
        let mut agents = Vec::new();
        let mut group_agents = Vec::new();
        for agent in &project.agents {
            let card = &agent.card;
            let listed_name = agent.name.clone();
            let listed_state = card.state.clone();
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
                    p.current_step = p.current_step.map(|s| super::excerpt(&s, 60));
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
                        super::excerpt(&agent.name, 30),
                        super::excerpt(&item.approval.tool, 32)
                    ));
                    target.request_id = Some(t.request_id.clone());
                    result.attention.push(WorkspaceActivityAttention {
                        target: target.clone(),
                        kind: "approval".into(),
                        project: row.name.clone(),
                        agent: super::excerpt(&agent.name, 30),
                        reason: reason.clone().unwrap_or_default(),
                    });
                    pending.push(WorkspaceActivityPendingItem {
                        kind: "approval".into(),
                        area_id: project.area_id.clone(),
                        node_id: card.node_id,
                        project: row.name.clone(),
                        agent: super::excerpt(&agent.name, 30),
                        reason: reason.clone().unwrap_or_default(),
                        request_id: Some(t.request_id.clone()),
                        fingerprint: None,
                        run_id: target.run_id.clone(),
                        session_id: target.session_id.clone(),
                        turn_id: target.turn_id.clone(),
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
                        agent: super::excerpt(&agent.name, 30),
                        reason: super::excerpt(
                            card.task_summary.as_deref().unwrap_or("执行失败，查看原因"),
                            80,
                        ),
                    });
                }
                if priority > WorkspaceActivityStatus::Failed {
                    priority = WorkspaceActivityStatus::Failed;
                    reason = Some(format!(
                        "{} · {}",
                        super::excerpt(&agent.name, 30),
                        super::excerpt(
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
                    && seen_reviews.insert((project.area_id.clone(), review.fingerprint.clone()))
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
                        agent: super::excerpt(&agent.name, 30),
                        reason: format!("{} 个文件待验收", review.file_count),
                    });
                    pending.push(WorkspaceActivityPendingItem {
                        kind: "review".into(),
                        area_id: project.area_id.clone(),
                        node_id: card.node_id,
                        project: row.name.clone(),
                        agent: super::excerpt(&agent.name, 30),
                        reason: format!("{} 个文件待验收", review.file_count),
                        request_id: None,
                        fingerprint: Some(review.fingerprint.clone()),
                        run_id: review.run_id.clone(),
                        session_id: review.session_id.clone(),
                        turn_id: review.turn_id.clone(),
                    });
                    if priority > WorkspaceActivityStatus::Review {
                        priority = WorkspaceActivityStatus::Review;
                        reason = Some(format!(
                            "{} · {} 个文件待验收",
                            super::excerpt(&agent.name, 30),
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
                    super::excerpt(&agent.name, 30),
                    super::excerpt(
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
            group_agents.push(WorkspaceActivityAgentEntry {
                area_id: project.area_id.clone(),
                node_id: card.node_id,
                name: listed_name,
                raw_state: listed_state,
                status: priority,
                task_title: card.task_title.clone(),
                started_at_unix_ms: card.task_timing.map(|timing| timing.started_at_unix_ms),
                run_id: card.run_id.clone(),
                session_id: card.session_id.clone(),
                turn_id: card.turn_id.clone(),
            });
            agents.push((
                priority,
                WorkspaceActivityAgent {
                    node_id: card.node_id,
                    name: super::excerpt(&agent.name, 30),
                    state: card.state.clone(),
                    action: super::excerpt(
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
        listed.extend(group_agents.iter().cloned());
        groups.push(WorkspaceActivityProjectGroup {
            area_id: row.area_id.clone(),
            project_id: row.project_id.clone(),
            name: row.name.clone(),
            status: row.status,
            agents: group_agents,
        });
        result.projects.push(row);
    }
    result.active |= result.approval_count + result.review_count + result.failure_count > 0;
    result.projects.sort_by_key(|p| p.status);
    (
        result,
        WorkspaceActivityView {
            agents: listed,
            pending,
            groups,
        },
    )
}
