//! Capsule derived from one workspace activity projection. No storage or UI.

use super::WorkspaceActivityFacts;

/// Sort order for the canvas status capsule: waiting, then failure, then running, then not started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatusCapsuleRank {
    Waiting,
    Failed,
    Running,
    PendingStart,
}

/// Execution word shared by the capsule and the canvas card title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusCapsuleKind {
    Waiting,
    Failed,
    Running,
    PendingStart,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusCapsuleChip {
    pub area_id: String,
    pub node_id: u64,
    pub display_name: String,
    pub rank: StatusCapsuleRank,
    pub status_label: String,
    pub count_label: String,
    pub title_label: String,
    pub completed: Option<u32>,
    pub total: Option<u32>,
    pub waiting_mark: bool,
    pub accessibility_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusCapsuleTitle {
    pub area_id: String,
    pub node_id: u64,
    pub title_label: String,
    pub completed: Option<u32>,
    pub total: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusCapsule {
    pub current_area_id: String,
    pub chips: Vec<StatusCapsuleChip>,
    pub titles: Vec<StatusCapsuleTitle>,
    pub overflow: usize,
    pub pending_count: usize,
    pub accessibility_label: String,
}

/// Card-title copy. Progress is the agent's own completed/total, never an estimate.
pub fn status_capsule_title(
    kind: StatusCapsuleKind,
    completed: Option<u32>,
    total: Option<u32>,
) -> String {
    let word = status_word(kind);
    match counted(completed, total) {
        Some((done, total)) => format!("{word} {done}/{total}"),
        None => word.to_string(),
    }
}

/// Visible chips, overflow and the workspace pending count for one cached fact set.
pub fn status_capsule(facts: &WorkspaceActivityFacts, current_area_id: &str) -> StatusCapsule {
    let mut ranked = Vec::new();
    let mut idle = 0usize;
    let mut pending_count = 0usize;
    let mut index = 0usize;
    for project in &facts.workspace.projects {
        for agent in &project.agents {
            let card = &agent.card;
            let approvals = approval_items(facts, &project.area_id, card);
            let reviews = review_items(facts, &project.area_id, card);
            let state_waiting = matches!(
                card.state.as_str(),
                "waiting_approval" | "ego_awaiting_user" | "ego_user_controlling"
            );
            let waiting_items = approvals + reviews + usize::from(state_waiting && approvals == 0);
            pending_count += waiting_items;
            let (completed, total) = card
                .task_progress
                .as_ref()
                .and_then(|progress| counted(Some(progress.completed), Some(progress.total)))
                .map(|(done, total)| (Some(done), Some(total)))
                .unwrap_or((None, None));
            let running = matches!(
                card.state.as_str(),
                "thinking"
                    | "running_tool"
                    | "interrupting"
                    | "collecting"
                    | "ego_working"
                    | "ego_creating"
                    | "ego_resuming"
            );
            let failed = matches!(card.state.as_str(), "failed" | "ego_failed");
            let pending_start = matches!(card.state.as_str(), "preparing" | "ego_preparing");
            let waiting = waiting_items > 0;
            let kind = if waiting && running {
                StatusCapsuleKind::Running
            } else if failed && !waiting {
                StatusCapsuleKind::Failed
            } else if running {
                StatusCapsuleKind::Running
            } else if pending_start && !waiting {
                StatusCapsuleKind::PendingStart
            } else if waiting {
                StatusCapsuleKind::Waiting
            } else {
                idle += 1;
                index += 1;
                continue;
            };
            let rank = if waiting {
                StatusCapsuleRank::Waiting
            } else {
                match kind {
                    StatusCapsuleKind::Failed => StatusCapsuleRank::Failed,
                    StatusCapsuleKind::Running => StatusCapsuleRank::Running,
                    StatusCapsuleKind::PendingStart => StatusCapsuleRank::PendingStart,
                    StatusCapsuleKind::Waiting => StatusCapsuleRank::Waiting,
                }
            };
            let current =
                project.area_id == current_area_id || project.project_id == current_area_id;
            let status_label = status_word(kind).to_string();
            let count_label = match (completed, total) {
                (Some(done), Some(total)) => format!("{done}/{total}"),
                _ => status_label.clone(),
            };
            let title_label = status_capsule_title(kind, completed, total);
            let display_name = if current {
                agent.name.clone()
            } else {
                format!("{} · {}", project.name, agent.name)
            };
            let accessibility_label = agent_sentence(
                &display_name,
                &status_label,
                completed,
                total,
                waiting,
                waiting_items,
            );
            ranked.push(RankedAgent {
                index,
                current,
                area_id: project.area_id.clone(),
                node_id: card.node_id,
                display_name,
                rank,
                status_label,
                count_label,
                title_label,
                completed,
                total,
                waiting_mark: waiting,
                accessibility_label,
            });
            index += 1;
        }
    }
    ranked.sort_by(|left, right| {
        left.rank
            .cmp(&right.rank)
            .then_with(|| left.current.cmp(&right.current).reverse())
            .then_with(|| left.index.cmp(&right.index))
    });
    let shown = ranked.len().min(3);
    let overflow = ranked.len().saturating_sub(shown) + idle;
    let titles = ranked
        .iter()
        .map(|agent| StatusCapsuleTitle {
            area_id: agent.area_id.clone(),
            node_id: agent.node_id,
            title_label: agent.title_label.clone(),
            completed: agent.completed,
            total: agent.total,
        })
        .collect();
    let chips = ranked
        .iter()
        .take(shown)
        .map(|agent| StatusCapsuleChip {
            area_id: agent.area_id.clone(),
            node_id: agent.node_id,
            display_name: agent.display_name.clone(),
            rank: agent.rank,
            status_label: agent.status_label.clone(),
            count_label: agent.count_label.clone(),
            title_label: agent.title_label.clone(),
            completed: agent.completed,
            total: agent.total,
            waiting_mark: agent.waiting_mark,
            accessibility_label: agent.accessibility_label.clone(),
        })
        .collect();
    let mut spoken = ranked
        .iter()
        .map(|agent| agent.accessibility_label.clone())
        .collect::<Vec<_>>();
    spoken.push(format!("共 {pending_count} 项待处理"));
    StatusCapsule {
        current_area_id: current_area_id.to_string(),
        chips,
        titles,
        overflow,
        pending_count,
        accessibility_label: spoken.join("；"),
    }
}

struct RankedAgent {
    index: usize,
    current: bool,
    area_id: String,
    node_id: u64,
    display_name: String,
    rank: StatusCapsuleRank,
    status_label: String,
    count_label: String,
    title_label: String,
    completed: Option<u32>,
    total: Option<u32>,
    waiting_mark: bool,
    accessibility_label: String,
}

fn status_word(kind: StatusCapsuleKind) -> &'static str {
    match kind {
        StatusCapsuleKind::Waiting => "等你处理",
        StatusCapsuleKind::Failed => "出错",
        StatusCapsuleKind::Running => "运行中",
        StatusCapsuleKind::PendingStart => "待开始",
    }
}

fn counted(completed: Option<u32>, total: Option<u32>) -> Option<(u32, u32)> {
    match (completed, total) {
        (Some(done), Some(total)) if total > 0 && done <= total => Some((done, total)),
        _ => None,
    }
}

fn agent_sentence(
    name: &str,
    status: &str,
    completed: Option<u32>,
    total: Option<u32>,
    waiting: bool,
    waiting_items: usize,
) -> String {
    let mut sentence = format!("{name} {status}");
    if let Some((done, total)) = counted(completed, total) {
        sentence.push_str(&format!("，已完成 {done}/{total}"));
    }
    if waiting && status != "等你处理" {
        let items = waiting_items.max(1);
        sentence.push_str(&format!("，{items} 项待你处理"));
    }
    sentence
}

fn approval_items(
    facts: &WorkspaceActivityFacts,
    area_id: &str,
    card: &super::super::CardSnapshot,
) -> usize {
    facts
        .approvals
        .iter()
        .filter(|item| {
            item.area_id == area_id
                && item.approval.target.valid()
                && !item.approval.decisions.is_empty()
                && item.approval.target.node_id == card.node_id
                && card.profile == item.approval.target.profile
                && card.run_id.as_deref() == Some(item.approval.target.run_id.as_str())
                && card.session_id.as_deref() == Some(item.approval.target.session_id.as_str())
                && card.turn_id.as_deref() == Some(item.approval.target.turn_id.as_str())
        })
        .count()
}

fn review_items(
    facts: &WorkspaceActivityFacts,
    area_id: &str,
    card: &super::super::CardSnapshot,
) -> usize {
    facts
        .reviews
        .iter()
        .filter(|review| {
            review.area_id == area_id
                && review.node_id == card.node_id
                && review.profile == card.profile
                && review.completed
                && review.available
                && !review.conflicted
                && !review.accepted
        })
        .count()
}
