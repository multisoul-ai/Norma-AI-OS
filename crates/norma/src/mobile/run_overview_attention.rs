//! Rows of the "needs your attention" section: failed agents, approvals, takeovers, reviews, files.
//! Split out of `run_overview.rs`, which groups agents by state.

use super::{
    is_loaded, presence_of, RunOverviewAttention, RunOverviewAttentionKind, RunOverviewFile,
    RunOverviewPresence, RunOverviewProjectPresence,
};
use crate::mobile::WorkspaceActivityView;

pub(super) fn attention_rows(
    view: &WorkspaceActivityView,
    files: &[RunOverviewFile],
    presence: &[RunOverviewProjectPresence],
    current_area_id: &str,
) -> Vec<RunOverviewAttention> {
    // 与状态分组同一个口径（见 `is_loaded`）。
    let loaded = |area_id: &str| is_loaded(presence, current_area_id, area_id);
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
                loaded: loaded(&agent.area_id),
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
            loaded: loaded(&item.area_id),
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
                loaded: loaded(&agent.area_id),
            });
        }
    }
    // Ego agents waiting for the user to take over: same urgency as an approval, but there is
    // no approval fact behind them, so every such agent gets its own row.
    for group in &view.groups {
        for agent in &group.agents {
            if agent.raw_state != "ego_awaiting_user" {
                continue;
            }
            rows.push(RunOverviewAttention {
                kind: RunOverviewAttentionKind::Takeover,
                area_id: agent.area_id.clone(),
                node_id: agent.node_id,
                project: group.name.clone(),
                agent: agent.name.clone(),
                title: agent
                    .task_title
                    .clone()
                    .filter(|title| !title.is_empty())
                    .unwrap_or_else(|| "等待你接管".into()),
                run_id: agent.run_id.clone().unwrap_or_default(),
                turn_id: agent.turn_id.clone().unwrap_or_default(),
                request_id: None,
                fingerprint: None,
                paths: Vec::new(),
                loaded: loaded(&agent.area_id),
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
            loaded: loaded(&item.area_id),
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
            loaded: loaded(&file.area_id),
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
            loaded: loaded(&file.area_id),
        });
    }
    rows
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
) -> Option<&'a crate::mobile::WorkspaceActivityAgentEntry> {
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

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}
