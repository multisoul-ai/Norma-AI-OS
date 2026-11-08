//! One face per loaded agent, plus the pending-card queue. No GPUI and no disk.

use super::{controller::alert_key, IslandProject, IslandWorkspace};
use norma::mobile::{RunOverviewAgent, RunOverviewAttention, RunOverviewAttentionKind};
use runtime::island::roster::{pending_successor, AgentActivity, SlotAgent, SlotKind};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Face {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub workspace_name: String,
    pub workspace_id: String,
    pub project_name: String,
    pub source: u64,
    pub area_id: String,
    pub node_id: u64,
    pub activity: AgentActivity,
    pub slot: SlotKind,
    pub task: String,
    pub detail: String,
    pub files: usize,
    pub progress: Option<(u32, u32)>,
    pub started_at_unix_ms: Option<u64>,
    completed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct QueueItem {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub workspace: String,
    pub review: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct QueueStep {
    pub focused_review: Option<String>,
    pub select_alert: Option<String>,
    pub hold_review: bool,
}

pub(super) fn review_id(source: u64, workspace: &str, area: &str, node: u64, turn: &str) -> String {
    format!("review:{source}:{workspace}:{area}:{node}:{turn}")
}

pub(super) fn agent_id(source: u64, area: &str, node: u64) -> String {
    format!("{source}:{area}:{node}")
}

/// Match the finished card, including its project. `{source}:{node}` is shared
/// by every project under one host, so it must not select a face.
pub(super) fn matching_completion_face<'a>(
    faces: &'a [Face],
    completion_id: &str,
) -> Option<&'a Face> {
    if completion_id.is_empty() {
        return None;
    }
    faces.iter().find(|face| face.id == completion_id)
}

/// Same id wins once. A pending card removes the agent from the running side.
/// An approval or a question means they are waiting. A result keeps the responded
/// face when the task is already done.
pub(super) fn collect(workspaces: &[IslandWorkspace]) -> Vec<Face> {
    let mut faces = Vec::new();
    let mut index: BTreeMap<(u64, String, u64), usize> = BTreeMap::new();
    for workspace in workspaces {
        for project in &workspace.projects {
            for agent in project
                .overview
                .running
                .iter()
                .chain(project.overview.idle.iter())
            {
                let key = (project.source, project.area_id.clone(), agent.node_id);
                if index.contains_key(&key) {
                    continue;
                }
                let running = project
                    .overview
                    .running
                    .iter()
                    .any(|row| row.node_id == agent.node_id);
                let completed = agent.is_completed();
                let activity = if running {
                    AgentActivity::Running
                } else if completed {
                    AgentActivity::Done
                } else {
                    AgentActivity::Queued
                };
                index.insert(key, faces.len());
                faces.push(face_from_agent(
                    workspace, project, agent, activity, completed,
                ));
            }
            for approval in &project.approvals {
                ensure_face(
                    &mut faces,
                    &mut index,
                    workspace,
                    project,
                    SyntheticFace {
                        node: approval.target.node_id,
                        name: &approval.card_title,
                        provider: &approval.target.profile,
                        completed: false,
                    },
                );
                mark(&mut faces, &index, project, approval.target.node_id, false);
            }
            for question in &project.questions {
                let provider = format!("{:?}", question.snapshot.question.target.provider);
                ensure_face(
                    &mut faces,
                    &mut index,
                    workspace,
                    project,
                    SyntheticFace {
                        node: question.snapshot.question.target.node_id,
                        name: &question.agent,
                        provider: &provider,
                        completed: false,
                    },
                );
                mark(
                    &mut faces,
                    &index,
                    project,
                    question.snapshot.question.target.node_id,
                    false,
                );
            }
            for row in &project.overview.attention {
                if !is_result(row.kind) {
                    continue;
                }
                ensure_face(
                    &mut faces,
                    &mut index,
                    workspace,
                    project,
                    SyntheticFace {
                        node: row.node_id,
                        name: &row.agent,
                        provider: &row.agent,
                        completed: true,
                    },
                );
                if let Some(slot) =
                    index.get(&(project.source, project.area_id.clone(), row.node_id))
                {
                    faces[*slot].files = faces[*slot].files.max(row.paths.len());
                }
                mark(&mut faces, &index, project, row.node_id, true);
            }
        }
    }
    faces
}

pub(super) fn slots(faces: &[Face]) -> Vec<SlotAgent> {
    faces
        .iter()
        .map(|face| SlotAgent {
            id: face.id.clone(),
            slot: face.slot,
        })
        .collect()
}

/// One card per turn. A Review already lists that turn's files, so those File
/// rows are not extra cards. A File row with no Review stays its own card.
pub(super) fn result_cards(rows: &[RunOverviewAttention]) -> Vec<&RunOverviewAttention> {
    let covered: BTreeSet<(u64, &str)> = rows
        .iter()
        .filter(|row| row.kind == RunOverviewAttentionKind::Review)
        .map(|row| (row.node_id, row.turn_id.as_str()))
        .collect();
    let mut seen = BTreeSet::new();
    let mut cards = Vec::new();
    for row in rows {
        if !is_result(row.kind) {
            continue;
        }
        let key = (row.node_id, row.turn_id.as_str());
        if row.kind == RunOverviewAttentionKind::File && covered.contains(&key) {
            continue;
        }
        if !seen.insert((row.node_id, row.turn_id.clone())) {
            continue;
        }
        cards.push(row);
    }
    cards
}

pub(super) fn queue_items(workspaces: &[IslandWorkspace]) -> Vec<QueueItem> {
    let mut items = Vec::new();
    for workspace in workspaces {
        for project in &workspace.projects {
            for approval in &project.approvals {
                items.push(QueueItem {
                    id: alert_key(
                        project.source,
                        &workspace.id,
                        &project.area_id,
                        &approval.target,
                    ),
                    name: approval.card_title.clone(),
                    provider: approval.target.profile.clone(),
                    workspace: workspace.name.clone(),
                    review: false,
                });
            }
            for question in &project.questions {
                items.push(QueueItem {
                    id: alert_key(
                        project.source,
                        &workspace.id,
                        &project.area_id,
                        &question.snapshot.question.target,
                    ),
                    name: question.agent.clone(),
                    provider: format!("{:?}", question.snapshot.question.target.provider),
                    workspace: workspace.name.clone(),
                    review: false,
                });
            }
            for row in result_cards(&project.overview.attention) {
                items.push(QueueItem {
                    id: review_id(
                        project.source,
                        &workspace.id,
                        &project.area_id,
                        row.node_id,
                        &row.turn_id,
                    ),
                    name: row.agent.clone(),
                    provider: row.agent.clone(),
                    workspace: workspace.name.clone(),
                    review: true,
                });
            }
        }
    }
    items
}

/// The card that just left yields the next one. A result stays in the alert shell.
pub(super) fn queue_step(
    previous: &[String],
    remaining: &[QueueItem],
    current: Option<&str>,
) -> QueueStep {
    let Some(current) = current else {
        return QueueStep {
            focused_review: None,
            select_alert: None,
            hold_review: false,
        };
    };
    let ids: Vec<String> = remaining.iter().map(|item| item.id.clone()).collect();
    let Some(next) = pending_successor(previous, &ids, current) else {
        return QueueStep {
            focused_review: None,
            select_alert: None,
            hold_review: false,
        };
    };
    let review = remaining.iter().any(|item| item.id == next && item.review);
    if review {
        QueueStep {
            focused_review: Some(next),
            select_alert: None,
            hold_review: true,
        }
    } else if remaining.iter().any(|item| item.id == next) && next != current {
        QueueStep {
            focused_review: None,
            select_alert: Some(next),
            hold_review: false,
        }
    } else if remaining
        .iter()
        .any(|item| item.id == current && !item.review)
    {
        QueueStep {
            focused_review: None,
            select_alert: None,
            hold_review: false,
        }
    } else {
        QueueStep {
            focused_review: remaining
                .iter()
                .find(|item| item.id == current && item.review)
                .map(|item| item.id.clone()),
            select_alert: None,
            hold_review: remaining
                .iter()
                .any(|item| item.id == current && item.review),
        }
    }
}

fn is_result(kind: RunOverviewAttentionKind) -> bool {
    matches!(
        kind,
        RunOverviewAttentionKind::Review | RunOverviewAttentionKind::File
    )
}

fn face_from_agent(
    workspace: &IslandWorkspace,
    project: &IslandProject,
    agent: &RunOverviewAgent,
    activity: AgentActivity,
    completed: bool,
) -> Face {
    let slot = if activity == AgentActivity::Running {
        SlotKind::Running
    } else {
        SlotKind::Hidden
    };
    Face {
        id: agent_id(project.source, &project.area_id, agent.node_id),
        name: agent.name.clone(),
        provider: agent.profile.clone(),
        workspace_name: workspace.name.clone(),
        workspace_id: workspace.id.clone(),
        project_name: project.name.clone(),
        source: project.source,
        area_id: project.area_id.clone(),
        node_id: agent.node_id,
        activity,
        slot,
        task: agent
            .latest_prompt
            .clone()
            .or_else(|| agent.task.clone())
            .unwrap_or_default(),
        detail: agent.action.clone().unwrap_or_default(),
        files: 0,
        progress: agent
            .progress
            .as_ref()
            .map(|progress| (progress.completed, progress.total)),
        started_at_unix_ms: agent.started_at_unix_ms,
        completed,
    }
}

struct SyntheticFace<'a> {
    node: u64,
    name: &'a str,
    provider: &'a str,
    completed: bool,
}

fn ensure_face(
    faces: &mut Vec<Face>,
    index: &mut BTreeMap<(u64, String, u64), usize>,
    workspace: &IslandWorkspace,
    project: &IslandProject,
    synthetic: SyntheticFace<'_>,
) {
    let key = (project.source, project.area_id.clone(), synthetic.node);
    if index.contains_key(&key) {
        return;
    }
    index.insert(key, faces.len());
    faces.push(Face {
        id: agent_id(project.source, &project.area_id, synthetic.node),
        name: synthetic.name.to_owned(),
        provider: synthetic.provider.to_owned(),
        workspace_name: workspace.name.clone(),
        workspace_id: workspace.id.clone(),
        project_name: project.name.clone(),
        source: project.source,
        area_id: project.area_id.clone(),
        node_id: synthetic.node,
        activity: AgentActivity::Queued,
        slot: SlotKind::Hidden,
        task: String::new(),
        detail: String::new(),
        files: 0,
        progress: None,
        started_at_unix_ms: None,
        completed: synthetic.completed,
    });
}

fn mark(
    faces: &mut [Face],
    index: &BTreeMap<(u64, String, u64), usize>,
    project: &IslandProject,
    node: u64,
    review: bool,
) {
    let Some(slot) = index.get(&(project.source, project.area_id.clone(), node)) else {
        return;
    };
    let face = &mut faces[*slot];
    face.slot = SlotKind::Pending;
    let user_is_waiting = !review;
    if user_is_waiting || !face.completed {
        face.activity = AgentActivity::Waiting;
    } else {
        face.activity = AgentActivity::Done;
    }
    if user_is_waiting {
        face.completed = false;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/island_faces.rs"]
mod tests;
