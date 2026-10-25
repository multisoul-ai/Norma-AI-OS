//! Desktop overview reads a cached projection. Recompute runs off the render path.
use super::status::{
    approval_is_actionable, approval_is_pending, ego_stop_agent, live_agents, state_label,
};
use crate::app::MultisoulApp;
use gpui::{App, Context};
use norma::mobile::{
    project_run_overview, status_capsule, CardSnapshot, DesktopChatAgent, DesktopChatProject,
    DesktopChatWorkspace, RunOverview, RunOverviewFile, RunOverviewProjectPresence, StatusCapsule,
    WorkspaceActivityFacts, WorkspaceActivitySummary, WorkspaceActivityView,
};
use runtime::RuntimeEvent;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
#[path = "../../../tests/unit/desktop_activity_overview_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActivityPresentation {
    pub area_id: String,
    pub node_id: u64,
    pub live: bool,
    pub updated_at_unix_ms: Option<i64>,
    pub ego_stop_agent: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OverviewAgentRow {
    pub area_id: String,
    pub node_id: u64,
    pub label: String,
    pub task: String,
    pub state: String,
    pub needs_approval: bool,
    pub can_locate_approval: bool,
    pub run_id: Option<String>,
    pub turn_id: Option<String>,
    pub stale: bool,
    pub is_ego: bool,
    pub ego_stop_agent: Option<u64>,
    pub updated_at_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OverviewArea {
    pub area_id: String,
    pub rows: Vec<OverviewAgentRow>,
}

pub(super) fn overview_areas_from_activity(
    area_ids: &[String],
    view: &WorkspaceActivityView,
    presentation: &[ActivityPresentation],
) -> Vec<OverviewArea> {
    area_ids
        .iter()
        .map(|area_id| {
            let agents = view
                .groups
                .iter()
                .find(|group| group.area_id == *area_id)
                .map(|group| group.agents.as_slice())
                .unwrap_or(&[]);
            OverviewArea {
                area_id: area_id.clone(),
                rows: agents
                    .iter()
                    .map(|agent| {
                        let side = presentation
                            .iter()
                            .find(|item| item.area_id == *area_id && item.node_id == agent.node_id);
                        let live = side.is_some_and(|item| item.live);
                        OverviewAgentRow {
                            area_id: area_id.clone(),
                            node_id: agent.node_id,
                            label: agent.name.clone(),
                            task: agent
                                .task_title
                                .clone()
                                .unwrap_or_else(|| "暂无任务标题".into()),
                            state: state_label(&agent.raw_state).into(),
                            needs_approval: approval_is_pending(&agent.raw_state),
                            can_locate_approval: approval_is_actionable(
                                live,
                                &agent.raw_state,
                                agent.run_id.as_deref(),
                                agent.turn_id.as_deref(),
                            ),
                            run_id: agent.run_id.clone(),
                            turn_id: agent.turn_id.clone(),
                            stale: !live,
                            is_ego: agent.raw_state.starts_with("ego_"),
                            ego_stop_agent: side
                                .filter(|item| item.live)
                                .and_then(|item| item.ego_stop_agent),
                            updated_at_unix_ms: side.and_then(|item| item.updated_at_unix_ms),
                        }
                    })
                    .collect(),
            }
        })
        .collect()
}

struct ActivityInput {
    generation: u64,
    facts: WorkspaceActivityFacts,
    area_ids: Vec<String>,
    presentation: Vec<ActivityPresentation>,
    current_area_id: String,
    files: Vec<RunOverviewFile>,
    presence: Vec<RunOverviewProjectPresence>,
}

struct ActivityOutput {
    generation: u64,
    summary: WorkspaceActivitySummary,
    view: WorkspaceActivityView,
    areas: Vec<OverviewArea>,
    overview: RunOverview,
    capsule: StatusCapsule,
}

fn project_activity(input: ActivityInput) -> ActivityOutput {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let view = WorkspaceActivityView::from_facts(&input.facts);
    let mut capsule = status_capsule(&input.facts, &input.current_area_id);
    let summary = WorkspaceActivitySummary::from_facts("desktop", input.facts, now);
    let areas = overview_areas_from_activity(&input.area_ids, &view, &input.presentation);
    let overview =
        project_run_overview(&input.current_area_id, &view, &input.files, &input.presence);
    // 状态条与抽屉统计同源：数字都从这一份概览投影里来。
    capsule.apply_run_overview(&overview);
    ActivityOutput {
        generation: input.generation,
        summary,
        view,
        areas,
        overview,
        capsule,
    }
}

pub(crate) fn workspace_activity_event(event: &RuntimeEvent) -> bool {
    matches!(
        event,
        RuntimeEvent::AgentTaskStateChanged { .. }
            | RuntimeEvent::EgoTaskStateChanged { .. }
            | RuntimeEvent::NodeRegistered { .. }
            | RuntimeEvent::NodeRemoved { .. }
    )
}

impl MultisoulApp {
    pub(super) fn overview_rows_for(&self, area_id: &str) -> Vec<OverviewAgentRow> {
        self.desktop_workspace
            .activity_overview
            .as_ref()
            .and_then(|areas| areas.iter().find(|area| area.area_id == area_id))
            .map(|area| area.rows.clone())
            .unwrap_or_default()
    }

    pub(super) fn invalidate_workspace_activity(&mut self) {
        self.desktop_workspace.activity_generation =
            self.desktop_workspace.activity_generation.wrapping_add(1);
        self.desktop_workspace.activity_summary = None;
        self.desktop_workspace.activity_view = None;
        self.desktop_workspace.activity_overview = None;
        self.desktop_workspace.activity_run_overview = None;
        self.desktop_workspace.activity_capsule = None;
        self.desktop_workspace.overview_cursor = None;
        self.desktop_workspace.overview_prelight = None;
    }

    pub(super) fn ensure_workspace_activity_cache(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.active_id.is_none() {
            return;
        }
        let current = self
            .desktop_workspace
            .expanded_area_id
            .clone()
            .or_else(|| self.desktop_workspace.active_area_id.clone())
            .unwrap_or_default();
        let fresh = self.desktop_workspace.activity_overview.is_some()
            && self.desktop_workspace.activity_run_overview.is_some()
            && self
                .desktop_workspace
                .activity_capsule
                .as_ref()
                .is_some_and(|capsule| capsule.current_area_id == current);
        if fresh {
            return;
        }
        self.mark_workspace_activity_dirty(cx);
    }

    pub(crate) fn note_workspace_activity_from_frame(
        &mut self,
        hook_changed: bool,
        events: &[RuntimeEvent],
        cx: &mut Context<Self>,
    ) {
        if hook_changed || events.iter().any(workspace_activity_event) {
            self.note_workspace_activity_changed(cx);
        }
    }

    pub(super) fn mark_workspace_activity_dirty(&mut self, cx: &mut Context<Self>) {
        if self.desktop_workspace.activity_gate.mark() {
            self.spawn_workspace_activity_refresh(cx);
        }
    }

    pub(crate) fn note_workspace_activity_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            let _ = host.update(cx, |host, cx| host.mark_workspace_activity_dirty(cx));
            return;
        }
        if matches!(self.screen, crate::app::AppScreen::DesktopWorkspace) {
            self.mark_workspace_activity_dirty(cx);
        }
    }

    fn spawn_workspace_activity_refresh(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Some(input) = this
                .update(cx, |app, cx| {
                    app.desktop_workspace.activity_gate.begin();
                    app.capture_activity_input(cx)
                })
                .ok()
                .flatten()
            else {
                let _ = this.update(cx, |app, cx| {
                    if app.desktop_workspace.activity_gate.finish() {
                        app.spawn_workspace_activity_refresh(cx);
                    }
                });
                return;
            };
            let projected = cx
                .background_executor()
                .spawn(async move { project_activity(input) })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.activity_generation == projected.generation {
                    app.desktop_workspace.activity_summary = Some(projected.summary);
                    app.desktop_workspace.activity_view = Some(projected.view);
                    app.desktop_workspace.activity_overview = Some(projected.areas);
                    app.desktop_workspace.activity_run_overview = Some(projected.overview.clone());
                    app.desktop_workspace.activity_updated_at_ms = Some(overview_clock_ms() as i64);
                    let count = projected.overview.attention.len()
                        + projected.overview.running.len()
                        + projected.overview.idle.len()
                        + projected.overview.unloaded_projects.len();
                    if app
                        .desktop_workspace
                        .overview_cursor
                        .is_some_and(|index| index >= count)
                    {
                        app.desktop_workspace.overview_cursor = count.checked_sub(1);
                    }
                    app.desktop_workspace.activity_capsule = Some(projected.capsule);
                    // 智能体状态变了：已指派的想法要跟着任务状态联动（完成、取消、失败）。
                    if !app.desktop_workspace.ideas.is_empty() {
                        app.refresh_ideas(cx);
                    }
                    cx.notify();
                }
                if app.desktop_workspace.activity_gate.finish() {
                    app.spawn_workspace_activity_refresh(cx);
                }
            });
        })
        .detach();
    }

    fn capture_activity_input(&self, cx: &App) -> Option<ActivityInput> {
        let workspace_id = self.desktop_workspace.active_id.clone()?;
        let workspace_name = self
            .desktop_workspace
            .active()
            .map(|workspace| workspace.name.clone())
            .unwrap_or_default();
        let mut projects = Vec::new();
        let mut presentation = Vec::new();
        let mut area_ids = Vec::new();
        for area in &self.desktop_workspace.areas {
            area_ids.push(area.id.clone());
            let (agents, presented) = self.capture_area_agents(&workspace_id, &area.id, cx);
            presentation.extend(presented);
            projects.push(DesktopChatProject {
                area_id: area.id.clone(),
                project_id: area.id.clone(),
                name: area.display_name.clone(),
                root: String::new(),
                agents,
                definitions: Vec::new(),
            });
        }
        let current_area_id = self
            .desktop_workspace
            .expanded_area_id
            .clone()
            .or_else(|| self.desktop_workspace.active_area_id.clone())
            .unwrap_or_default();
        let files = self
            .desktop_workspace
            .content_artifacts
            .iter()
            .map(|artifact| RunOverviewFile {
                area_id: artifact.area_id.clone(),
                node_id: artifact.node_id,
                agent_hint: artifact.agent_profile.clone(),
                project_hint: self
                    .desktop_workspace
                    .areas
                    .iter()
                    .find(|area| area.id == artifact.area_id)
                    .map(|area| area.display_name.clone())
                    .unwrap_or_default(),
                path: artifact.source_path.clone(),
                turn_id: artifact.turn_id.clone(),
                fingerprint: artifact.version.clone(),
            })
            .collect();
        let presence = self
            .desktop_workspace
            .areas
            .iter()
            .map(|area| RunOverviewProjectPresence {
                area_id: area.id.clone(),
                loaded: self
                    .desktop_workspace
                    .sessions
                    .contains_key(&(workspace_id.clone(), area.id.clone())),
                updated_at_unix_ms: self
                    .desktop_workspace
                    .status_cache
                    .get(&area.id)
                    .map(|status| status.updated_at_unix_ms),
            })
            .collect();
        Some(ActivityInput {
            generation: self.desktop_workspace.activity_generation,
            area_ids,
            presentation,
            current_area_id,
            files,
            presence,
            facts: WorkspaceActivityFacts {
                workspace: DesktopChatWorkspace {
                    id: workspace_id,
                    name: workspace_name,
                    revision: 0,
                    projects,
                },
                observed: true,
                approvals: Vec::new(),
                reviews: Vec::new(),
            },
        })
    }

    fn capture_area_agents(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &App,
    ) -> (Vec<DesktopChatAgent>, Vec<ActivityPresentation>) {
        let view = self.desktop_area_status_view(workspace_id, area_id, cx);
        let mut agents = Vec::new();
        let mut presentation = Vec::new();
        if view.live {
            if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.to_owned(), area_id.to_owned()))
            {
                let project = session.read(cx);
                let snapshots = project.mobile_card_snapshots();
                for agent in live_agents(&project) {
                    let card = card_from_status(
                        &agent,
                        snapshots.iter().find(|card| card.node_id == agent.node_id),
                    );
                    presentation.push(ActivityPresentation {
                        area_id: area_id.to_owned(),
                        node_id: agent.node_id,
                        live: true,
                        updated_at_unix_ms: None,
                        ego_stop_agent: agent
                            .state
                            .starts_with("ego_")
                            .then(|| ego_stop_agent(&project, agent.node_id))
                            .flatten(),
                    });
                    agents.push(chat_agent(agent.name, card));
                }
                return (agents, presentation);
            }
        }
        for agent in view.agents {
            let node_id = agent.node_id;
            let name = agent.name.clone();
            presentation.push(ActivityPresentation {
                area_id: area_id.to_owned(),
                node_id,
                live: false,
                updated_at_unix_ms: view.updated_at_unix_ms,
                ego_stop_agent: None,
            });
            agents.push(chat_agent(name, card_from_status(&agent, None)));
        }
        (agents, presentation)
    }
}

fn chat_agent(name: String, card: CardSnapshot) -> DesktopChatAgent {
    let profile = card.profile.clone();
    DesktopChatAgent {
        definition_id: 0,
        name,
        profile,
        card,
    }
}

fn card_from_status(
    agent: &storage::DesktopAgentStatus,
    snapshot: Option<&CardSnapshot>,
) -> CardSnapshot {
    let mut card = snapshot.cloned().unwrap_or(CardSnapshot {
        node_id: agent.node_id,
        profile: "Agent".into(),
        title: agent.name.clone(),
        mobile_control: false,
        state: agent.state.clone(),
        run_id: agent.run_id.clone(),
        session_id: None,
        turn_id: agent.turn_id.clone(),
        task_title: agent.task.clone(),
        latest_prompt: None,
        can_interrupt: None,
        task_timing: None,
        task_progress: None,
        task_summary: None,
    });
    card.state = agent.state.clone();
    card.task_title = agent.task.clone();
    card.run_id = agent.run_id.clone();
    card.turn_id = agent.turn_id.clone();
    card
}

pub(in crate::workspace::desktop) fn overview_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}
