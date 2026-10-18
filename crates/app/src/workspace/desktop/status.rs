//! A single live/status-cache projection for the overview and voice replies.

use crate::app::MultisoulApp;
use canvas::{BrowserSurfaceKind, CanvasNodeKind, EgoCardPhase, NodeId};
use gpui::{App, Context};
use runtime::AgentTaskState;
use std::time::{SystemTime, UNIX_EPOCH};
use storage::{DesktopAgentStatus, DesktopAreaStatus, DesktopAreaStatusStore};

#[cfg(test)]
#[path = "../../../tests/unit/desktop_approval_identity_tests.rs"]
mod approval_identity_tests;

pub(super) fn approval_is_actionable(
    live: bool,
    state: &str,
    run_id: Option<&str>,
    turn_id: Option<&str>,
) -> bool {
    live && state == "waiting_approval"
        && run_id.is_some_and(|id| !id.is_empty())
        && turn_id.is_some_and(|id| !id.is_empty())
}

pub(super) fn approval_is_pending(state: &str) -> bool {
    state == "waiting_approval"
}

pub(super) fn approval_request_matches(
    state: &str,
    run_id: Option<&str>,
    turn_id: Option<&str>,
    expected_run: Option<&str>,
    expected_turn: Option<&str>,
) -> bool {
    approval_is_actionable(true, state, run_id, turn_id)
        && expected_run.is_some_and(|id| Some(id) == run_id)
        && expected_turn.is_some_and(|id| Some(id) == turn_id)
}

pub(super) struct DesktopAreaStatusView {
    pub agents: Vec<DesktopAgentStatus>,
    pub live: bool,
    pub updated_at_unix_ms: Option<i64>,
}

pub(super) struct AgentRow {
    pub area_id: String,
    pub node_id: NodeId,
    pub label: String,
    pub task: String,
    pub state: String,
    pub needs_approval: bool,
    pub can_locate_approval: bool,
    pub run_id: Option<String>,
    pub turn_id: Option<String>,
    pub stale: bool,
    pub is_ego: bool,
    pub ego_stop_agent: Option<NodeId>,
}

impl MultisoulApp {
    pub(super) fn desktop_area_status_view(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &App,
    ) -> DesktopAreaStatusView {
        if let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))
        {
            return DesktopAreaStatusView {
                agents: live_agents(&session.read(cx)),
                live: true,
                updated_at_unix_ms: None,
            };
        }
        if let Some(cached) = self.desktop_workspace.status_cache.get(area_id) {
            return DesktopAreaStatusView {
                agents: cached.agents.clone(),
                live: false,
                updated_at_unix_ms: Some(cached.updated_at_unix_ms),
            };
        }
        DesktopAreaStatusView {
            agents: Vec::new(),
            live: false,
            updated_at_unix_ms: None,
        }
    }

    pub(super) fn desktop_agent_rows(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &App,
    ) -> Vec<AgentRow> {
        let view = self.desktop_area_status_view(workspace_id, area_id, cx);
        let last_update = view.updated_at_unix_ms.map(last_update_label);
        view.agents
            .into_iter()
            .map(|agent| {
                let stale = !view.live;
                let needs_approval = approval_is_pending(&agent.state);
                let can_locate_approval = approval_is_actionable(
                    view.live,
                    &agent.state,
                    agent.run_id.as_deref(),
                    agent.turn_id.as_deref(),
                );
                let is_ego = agent.state.starts_with("ego_");
                let ego_stop_agent = if is_ego && view.live {
                    self.desktop_workspace
                        .sessions
                        .get(&(workspace_id.to_owned(), area_id.to_owned()))
                        .and_then(|session| {
                            let project = session.read(cx);
                            let state = project.runtime.ego_task_state(agent.node_id)?;
                            let bound = state.bound_agent_node_id?;
                            (matches!(
                                state.phase,
                                EgoCardPhase::PreparingAgent
                                    | EgoCardPhase::AgentWorking
                                    | EgoCardPhase::ResumeRequested
                            ) && matches!(
                                project.runtime.agent_task_state(bound),
                                Some(
                                    AgentTaskState::Thinking
                                        | AgentTaskState::RunningTool { .. }
                                        | AgentTaskState::WaitingApproval { .. }
                                )
                            ))
                            .then_some(bound)
                        })
                } else {
                    None
                };
                let task = agent.task.unwrap_or_else(|| "暂无任务标题".into());
                AgentRow {
                    area_id: area_id.to_owned(),
                    node_id: agent.node_id,
                    label: agent.name,
                    task: match &last_update {
                        Some(label) => format!("{task} · 上次更新{label}"),
                        None => task,
                    },
                    state: state_label(&agent.state).into(),
                    needs_approval,
                    can_locate_approval,
                    run_id: agent.run_id,
                    turn_id: agent.turn_id,
                    stale,
                    is_ego,
                    ego_stop_agent,
                }
            })
            .collect()
    }

    pub(super) fn refresh_desktop_agent_status_cache(&mut self, cx: &mut Context<Self>) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let mut changed = Vec::new();
        for area in &self.desktop_workspace.areas {
            let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.clone(), area.id.clone()))
            else {
                continue;
            };
            let agents = live_agents(&session.read(cx));
            let seen = self.desktop_workspace.status_live_seen.contains(&area.id);
            let old = self.desktop_workspace.status_cache.get(&area.id);
            if seen && old.is_some_and(|old| old.agents == agents) {
                continue;
            }
            let now = current_unix_ms();
            let updated_at_unix_ms =
                now.max(old.map_or(0, |old| old.updated_at_unix_ms.saturating_add(1)));
            let snapshot = DesktopAreaStatus {
                area_id: area.id.clone(),
                updated_at_unix_ms,
                agents,
            };
            self.desktop_workspace
                .status_cache
                .insert(area.id.clone(), snapshot.clone());
            self.desktop_workspace
                .status_live_seen
                .insert(area.id.clone());
            changed.push(snapshot);
        }
        if changed.is_empty() {
            return;
        }
        cx.notify();
        let store = DesktopAreaStatusStore::new(self.database_paths.app_db_path().to_owned());
        cx.spawn(async move |_, cx| {
            let _ = cx
                .background_executor()
                .spawn(async move {
                    for status in changed {
                        if let Err(error) = store.save(
                            &workspace_id,
                            &status.area_id,
                            status.updated_at_unix_ms,
                            &status.agents,
                        ) {
                            tracing::warn!(%error, area_id=%status.area_id, "desktop status cache save failed");
                        }
                    }
                })
                .await;
        })
        .detach();
    }
}

pub(super) fn live_agents(project: &MultisoulApp) -> Vec<DesktopAgentStatus> {
    let snapshots = project.mobile_card_snapshots();
    project
        .runtime
        .document
        .nodes()
        .iter()
        .filter_map(|node| match &node.kind {
            CanvasNodeKind::Agent(spec) => {
                let snapshot = snapshots.iter().find(|card| card.node_id == node.id);
                let fallback_state = match project.runtime.agent_task_state(node.id) {
                    Some(AgentTaskState::Ready) => "ready",
                    Some(AgentTaskState::Thinking) => "thinking",
                    Some(AgentTaskState::RunningTool { .. }) => "running_tool",
                    Some(AgentTaskState::WaitingApproval { .. }) => "waiting_approval",
                    Some(AgentTaskState::Done) => "done",
                    Some(AgentTaskState::Failed { .. }) => "failed",
                    Some(AgentTaskState::Ended) => "ended",
                    None => "not_running",
                };
                let state = snapshot.map_or(fallback_state, |card| card.state.as_str());
                Some(DesktopAgentStatus {
                    node_id: node.id,
                    name: spec
                        .alias
                        .clone()
                        .unwrap_or_else(|| spec.profile.display_title().to_string()),
                    task: project.runtime.agent_task_title(node.id),
                    state: state.into(),
                    run_id: snapshot.and_then(|card| card.run_id.clone()),
                    turn_id: snapshot.and_then(|card| card.turn_id.clone()),
                })
            }
            CanvasNodeKind::Browser(spec) => {
                let BrowserSurfaceKind::EgoLite(ego) = &spec.surface else {
                    return None;
                };
                let phase = project
                    .runtime
                    .ego_task_state(node.id)
                    .map(|state| state.phase)
                    .unwrap_or(ego.last_phase);
                let bound_agent_ended = project
                    .runtime
                    .ego_task_state(node.id)
                    .and_then(|state| state.bound_agent_node_id)
                    .is_some_and(|agent_id| {
                        matches!(
                            project.runtime.agent_task_state(agent_id),
                            Some(
                                AgentTaskState::Done
                                    | AgentTaskState::Failed { .. }
                                    | AgentTaskState::Ended
                            )
                        )
                    });
                let state = if bound_agent_ended
                    && matches!(
                        phase,
                        EgoCardPhase::PreparingAgent
                            | EgoCardPhase::AgentWorking
                            | EgoCardPhase::ResumeRequested
                    ) {
                    "ego_agent_stopped"
                } else {
                    ego_state_key(phase)
                };
                Some(DesktopAgentStatus {
                    node_id: node.id,
                    name: "Ego Lite".into(),
                    task: ego.task_space_name.clone().or(Some("网页任务".into())),
                    state: state.into(),
                    run_id: None,
                    turn_id: None,
                })
            }
            _ => None,
        })
        .collect()
}

pub(super) fn state_label(state: &str) -> &'static str {
    match state {
        "ready" => "待命",
        "thinking" => "思考中",
        "running_tool" => "执行中",
        "waiting_approval" => "待审核",
        "done" => "已完成",
        "failed" => "失败",
        "ended" => "已结束",
        "ego_creating" => "创建中",
        "ego_ready" => "待命",
        "ego_preparing" => "准备中",
        "ego_working" => "执行中",
        "ego_awaiting_user" => "等待接管",
        "ego_user_controlling" => "用户接管中",
        "ego_resuming" => "恢复中",
        "ego_completed" => "已完成",
        "ego_failed" => "失败",
        "ego_disconnected" => "已断开",
        "ego_agent_stopped" => "Agent 已结束，网页状态待同步",
        _ => "未运行",
    }
}

fn ego_state_key(phase: EgoCardPhase) -> &'static str {
    match phase {
        EgoCardPhase::Creating => "ego_creating",
        EgoCardPhase::ReadyForUser => "ego_ready",
        EgoCardPhase::PreparingAgent => "ego_preparing",
        EgoCardPhase::AgentWorking => "ego_working",
        EgoCardPhase::AwaitingUser => "ego_awaiting_user",
        EgoCardPhase::UserControlling => "ego_user_controlling",
        EgoCardPhase::ResumeRequested => "ego_resuming",
        EgoCardPhase::Completed => "ego_completed",
        EgoCardPhase::Failed => "ego_failed",
        EgoCardPhase::Disconnected => "ego_disconnected",
    }
}

pub(super) fn last_update_label(updated_at_unix_ms: i64) -> String {
    let age = current_unix_ms().saturating_sub(updated_at_unix_ms) / 1_000;
    match age {
        0..=59 => "刚刚".into(),
        60..=3_599 => format!("{} 分钟前", age / 60),
        3_600..=86_399 => format!("{} 小时前", age / 3_600),
        _ => format!("{} 天前", age / 86_400),
    }
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
