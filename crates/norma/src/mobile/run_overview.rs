//! Group a cached activity view into the run-overview sections: attention,
//! running agents, idle agents and unloaded-project summaries.
//! Elapsed time is not formatted here; the renderer converts `started_at_unix_ms`.

use super::{TaskProgress, WorkspaceActivityStatus, WorkspaceActivityView};

#[path = "run_overview_attention.rs"]
mod attention;
use attention::attention_rows;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOverviewFile {
    pub area_id: String,
    pub node_id: u64,
    pub agent_hint: String,
    pub project_hint: String,
    pub path: String,
    pub turn_id: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOverviewProjectPresence {
    pub area_id: String,
    pub loaded: bool,
    pub updated_at_unix_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOverviewAttentionKind {
    Failed,
    Approval,
    /// An Ego agent is blocked waiting for the user to take over.
    Takeover,
    Review,
    File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOverviewPresence {
    Running,
    Idle,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOverviewAttention {
    pub kind: RunOverviewAttentionKind,
    pub area_id: String,
    pub node_id: u64,
    pub project: String,
    pub agent: String,
    pub title: String,
    pub run_id: String,
    pub turn_id: String,
    pub request_id: Option<String>,
    pub fingerprint: Option<String>,
    pub paths: Vec<String>,
    pub loaded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOverviewAgent {
    pub area_id: String,
    pub project: String,
    /// True when the agent belongs to the project currently on screen.
    pub current: bool,
    pub node_id: u64,
    pub name: String,
    pub raw_state: String,
    pub presence: RunOverviewPresence,
    pub approval_count: u32,
    pub task: Option<String>,
    pub action: Option<String>,
    pub progress: Option<TaskProgress>,
    pub started_at_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOverviewProject {
    pub area_id: String,
    /// 1-based position among the workspace's projects, shown as the row's index box.
    pub order: usize,
    pub name: String,
    pub agent_count: usize,
    pub status: WorkspaceActivityStatus,
    pub loaded: bool,
    pub updated_at_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOverview {
    /// Failed agents first, then approvals, reviews and files, workspace-wide.
    pub attention: Vec<RunOverviewAttention>,
    /// Loaded projects' running agents; current project first, longest-running first.
    pub running: Vec<RunOverviewAgent>,
    /// Loaded projects' idle agents, ordered like `running`.
    pub idle: Vec<RunOverviewAgent>,
    /// Projects without a live runtime: one summary row each, never expanded.
    pub unloaded_projects: Vec<RunOverviewProject>,
}

/// Workspace-wide counts shown by the status capsule and the drawer's summary cards.
/// Agents count once per row; unloaded-project summary rows are not agents and are excluded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunOverviewSummary {
    pub attention: usize,
    pub running: usize,
    pub idle: usize,
}

impl RunOverview {
    pub fn summary(&self) -> RunOverviewSummary {
        RunOverviewSummary {
            attention: self.attention.len(),
            running: self.running.len(),
            idle: self.idle.len(),
        }
    }
}

pub fn project_run_overview(
    current_area_id: &str,
    view: &WorkspaceActivityView,
    files: &[RunOverviewFile],
    presence: &[RunOverviewProjectPresence],
) -> RunOverview {
    let attention = attention_rows(view, files, presence, current_area_id);
    let loaded = |area_id: &str| is_loaded(presence, current_area_id, area_id);
    let mut running = Vec::new();
    let mut idle = Vec::new();
    for group in view.groups.iter().filter(|group| loaded(&group.area_id)) {
        for agent in &group.agents {
            let mut row = agent_row(agent, &attention);
            row.project = group.name.clone();
            row.current = group.area_id == current_area_id;
            match row.presence {
                RunOverviewPresence::Running => running.push(row),
                RunOverviewPresence::Idle => idle.push(row),
                // Failed agents already own an attention row.
                RunOverviewPresence::Failed => {}
            }
        }
    }
    sort_agents(&mut running);
    sort_agents(&mut idle);
    let unloaded_projects = view
        .groups
        .iter()
        .enumerate()
        .filter(|(_, group)| !loaded(&group.area_id))
        .map(|(position, group)| {
            let side = presence.iter().find(|item| item.area_id == group.area_id);
            RunOverviewProject {
                area_id: group.area_id.clone(),
                order: position + 1,
                name: group.name.clone(),
                agent_count: group.agents.len(),
                status: project_status(group),
                loaded: false,
                updated_at_unix_ms: side.and_then(|item| item.updated_at_unix_ms),
            }
        })
        .collect();
    RunOverview {
        attention,
        running,
        idle,
        unloaded_projects,
    }
}

/// Current project first, then longest-running first; agents without a start time sort last.
/// The sort is stable, so equal keys keep the workspace order.
fn sort_agents(agents: &mut [RunOverviewAgent]) {
    agents.sort_by_key(|agent| (!agent.current, agent.started_at_unix_ms.unwrap_or(u64::MAX)));
}

fn project_status(group: &super::WorkspaceActivityProjectGroup) -> super::WorkspaceActivityStatus {
    group
        .agents
        .iter()
        .map(|agent| {
            if agent.raw_state == "waiting_approval" {
                super::WorkspaceActivityStatus::Approval
            } else {
                agent.status
            }
        })
        .min()
        .unwrap_or(group.status)
}

fn agent_row(
    agent: &super::WorkspaceActivityAgentEntry,
    attention: &[RunOverviewAttention],
) -> RunOverviewAgent {
    let presence = presence_of(&agent.raw_state);
    let progress = agent.progress.clone().filter(progress_is_usable);
    let action = progress
        .as_ref()
        .and_then(|item| item.current_step.clone())
        .or_else(|| {
            (presence == RunOverviewPresence::Running)
                .then(|| agent.task_title.clone())
                .flatten()
        });
    RunOverviewAgent {
        area_id: agent.area_id.clone(),
        project: String::new(),
        current: false,
        node_id: agent.node_id,
        name: agent.name.clone(),
        raw_state: agent.raw_state.clone(),
        presence,
        approval_count: attention
            .iter()
            .filter(|row| {
                row.kind == RunOverviewAttentionKind::Approval
                    && row.area_id == agent.area_id
                    && row.node_id == agent.node_id
            })
            .count() as u32,
        task: agent.task_title.clone(),
        action,
        progress,
        started_at_unix_ms: agent.started_at_unix_ms,
    }
}

fn presence_of(raw: &str) -> RunOverviewPresence {
    match raw {
        "failed" | "ego_failed" => RunOverviewPresence::Failed,
        "thinking" | "running_tool" | "preparing" | "interrupting" | "collecting"
        | "waiting_approval" | "ego_awaiting_user" | "ego_working" | "ego_preparing"
        | "ego_creating" | "ego_resuming" => RunOverviewPresence::Running,
        _ => RunOverviewPresence::Idle,
    }
}

fn progress_is_usable(progress: &TaskProgress) -> bool {
    progress.total > 0 && progress.completed <= progress.total
}

/// 状态分组与待处理行共用的“已加载”口径：当前项目一律视为已加载，其余项目看是否有会话。
fn is_loaded(
    presence: &[RunOverviewProjectPresence],
    current_area_id: &str,
    area_id: &str,
) -> bool {
    area_id == current_area_id
        || presence
            .iter()
            .find(|item| item.area_id == area_id)
            .is_some_and(|item| item.loaded)
}
