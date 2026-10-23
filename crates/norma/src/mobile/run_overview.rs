//! Group a cached activity view into the run-overview sections: attention,
//! running agents, idle agents and unloaded-project summaries.
//! Elapsed time is not formatted here; the renderer converts `started_at_unix_ms`.

use super::{TaskProgress, WorkspaceActivityStatus, WorkspaceActivityView};

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
    let attention = attention_rows(view, files, presence);
    let loaded = |area_id: &str| area_id == current_area_id || is_loaded(presence, area_id);
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

fn attention_rows(
    view: &WorkspaceActivityView,
    files: &[RunOverviewFile],
    presence: &[RunOverviewProjectPresence],
) -> Vec<RunOverviewAttention> {
    let mut rows = Vec::new();
    let mut covered = Vec::new();
    for group in &view.groups {
        for agent in &group.agents {
            if presence_of(&agent.raw_state) != RunOverviewPresence::Failed {
                continue;
            }
            rows.push(RunOverviewAttention {
                kind: RunOverviewAttentionKind::Failed,
                area_id: agent.area_id.clone(),
                node_id: agent.node_id,
                project: group.name.clone(),
                agent: agent.name.clone(),
                title: agent
                    .task_title
                    .clone()
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| "任务出错".into()),
                run_id: agent.run_id.clone().unwrap_or_default(),
                turn_id: agent.turn_id.clone().unwrap_or_default(),
                request_id: None,
                fingerprint: None,
                paths: Vec::new(),
                loaded: is_loaded(presence, &agent.area_id),
            });
        }
    }
    for item in view.pending.iter().filter(|item| item.kind == "approval") {
        covered.push((item.area_id.clone(), item.node_id));
        let agent = find_agent(view, &item.area_id, item.node_id);
        rows.push(RunOverviewAttention {
            kind: RunOverviewAttentionKind::Approval,
            area_id: item.area_id.clone(),
            node_id: item.node_id,
            project: item.project.clone(),
            agent: item.agent.clone(),
            title: approval_title(
                agent.and_then(|agent| agent.task_title.as_deref()),
                &item.reason,
            ),
            run_id: item.run_id.clone(),
            turn_id: item.turn_id.clone(),
            request_id: item.request_id.clone(),
            fingerprint: None,
            paths: Vec::new(),
            loaded: is_loaded(presence, &item.area_id),
        });
    }
    for group in &view.groups {
        for agent in &group.agents {
            if agent.raw_state != "waiting_approval"
                || covered
                    .iter()
                    .any(|item| item.0 == agent.area_id && item.1 == agent.node_id)
            {
                continue;
            }
            covered.push((agent.area_id.clone(), agent.node_id));
            rows.push(RunOverviewAttention {
                kind: RunOverviewAttentionKind::Approval,
                area_id: agent.area_id.clone(),
                node_id: agent.node_id,
                project: group.name.clone(),
                agent: agent.name.clone(),
                title: agent
                    .task_title
                    .clone()
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| "等待你确认".into()),
                run_id: agent.run_id.clone().unwrap_or_default(),
                turn_id: agent.turn_id.clone().unwrap_or_default(),
                request_id: None,
                fingerprint: None,
                paths: Vec::new(),
                loaded: is_loaded(presence, &agent.area_id),
            });
        }
    }
    let mut seen_reviews = Vec::new();
    for item in view.pending.iter().filter(|item| item.kind == "review") {
        seen_reviews.push((item.area_id.clone(), item.node_id, item.turn_id.clone()));
        let agent = find_agent(view, &item.area_id, item.node_id);
        rows.push(RunOverviewAttention {
            kind: RunOverviewAttentionKind::Review,
            area_id: item.area_id.clone(),
            node_id: item.node_id,
            project: item.project.clone(),
            agent: item.agent.clone(),
            title: agent
                .and_then(|agent| agent.task_title.clone())
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "本回合成果".into()),
            run_id: item.run_id.clone(),
            turn_id: item.turn_id.clone(),
            request_id: None,
            fingerprint: item.fingerprint.clone(),
            paths: paths_for(files, &item.area_id, item.node_id, &item.turn_id),
            loaded: is_loaded(presence, &item.area_id),
        });
    }
    for file in files {
        if file.path.is_empty()
            || seen_reviews.iter().any(|item| {
                item.0 == file.area_id && item.1 == file.node_id && item.2 == file.turn_id
            })
        {
            continue;
        }
        seen_reviews.push((file.area_id.clone(), file.node_id, file.turn_id.clone()));
        let agent = find_agent(view, &file.area_id, file.node_id);
        let group = view
            .groups
            .iter()
            .find(|group| group.area_id == file.area_id);
        rows.push(RunOverviewAttention {
            kind: RunOverviewAttentionKind::Review,
            area_id: file.area_id.clone(),
            node_id: file.node_id,
            project: group
                .map(|group| group.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| file.project_hint.clone()),
            agent: agent
                .map(|agent| agent.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| file.agent_hint.clone()),
            title: agent
                .and_then(|agent| agent.task_title.clone())
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "本回合成果".into()),
            run_id: agent
                .and_then(|agent| agent.run_id.clone())
                .unwrap_or_default(),
            turn_id: file.turn_id.clone(),
            request_id: None,
            fingerprint: Some(file.fingerprint.clone()),
            paths: paths_for(files, &file.area_id, file.node_id, &file.turn_id),
            loaded: is_loaded(presence, &file.area_id),
        });
    }
    for file in files.iter().filter(|file| !file.path.is_empty()) {
        let agent = find_agent(view, &file.area_id, file.node_id);
        let group = view
            .groups
            .iter()
            .find(|group| group.area_id == file.area_id);
        rows.push(RunOverviewAttention {
            kind: RunOverviewAttentionKind::File,
            area_id: file.area_id.clone(),
            node_id: file.node_id,
            project: group
                .map(|group| group.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| file.project_hint.clone()),
            agent: agent
                .map(|agent| agent.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| file.agent_hint.clone()),
            title: file_name(&file.path).to_string(),
            run_id: String::new(),
            turn_id: file.turn_id.clone(),
            request_id: None,
            fingerprint: Some(file.fingerprint.clone()),
            paths: vec![file.path.clone()],
            loaded: is_loaded(presence, &file.area_id),
        });
    }
    rows
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
        | "waiting_approval" | "ego_working" | "ego_preparing" | "ego_creating"
        | "ego_resuming" => RunOverviewPresence::Running,
        _ => RunOverviewPresence::Idle,
    }
}

fn progress_is_usable(progress: &TaskProgress) -> bool {
    progress.total > 0 && progress.completed <= progress.total
}

fn approval_title(task: Option<&str>, reason: &str) -> String {
    if let Some(task) = task.map(str::trim).filter(|task| !task.is_empty()) {
        return task.to_string();
    }
    reason
        .rsplit_once("确认 ")
        .map(|(_, tool)| tool.trim())
        .filter(|tool| !tool.is_empty())
        .unwrap_or("等待你确认")
        .to_string()
}

fn find_agent<'a>(
    view: &'a WorkspaceActivityView,
    area_id: &str,
    node_id: u64,
) -> Option<&'a super::WorkspaceActivityAgentEntry> {
    view.groups.iter().find_map(|group| {
        (group.area_id == area_id)
            .then(|| group.agents.iter().find(|agent| agent.node_id == node_id))
            .flatten()
    })
}

fn paths_for(files: &[RunOverviewFile], area_id: &str, node_id: u64, turn_id: &str) -> Vec<String> {
    files
        .iter()
        .filter(|file| {
            file.area_id == area_id
                && file.node_id == node_id
                && file.turn_id == turn_id
                && !file.path.is_empty()
        })
        .map(|file| file.path.clone())
        .collect()
}

fn is_loaded(presence: &[RunOverviewProjectPresence], area_id: &str) -> bool {
    presence
        .iter()
        .find(|item| item.area_id == area_id)
        .is_some_and(|item| item.loaded)
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}
