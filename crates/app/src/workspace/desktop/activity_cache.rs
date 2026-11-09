//! Desktop overview reads a cached projection. Recompute runs off the render path.
use super::status::{approval_is_actionable, approval_is_pending, state_label};
use crate::app::MultisoulApp;
use gpui::Context;
use norma::mobile::{
    project_run_overview, status_capsule, RunOverview, RunOverviewFile, RunOverviewProjectPresence,
    StatusCapsule, WorkspaceActivityFacts, WorkspaceActivitySummary, WorkspaceActivityView,
};
use runtime::RuntimeEvent;

mod capture;
mod cards;
mod current;
mod project;
pub(super) use capture::ActivityMonitor;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
#[path = "../../../tests/unit/desktop_activity_overview_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/island_activity_tests.rs"]
mod island_tests;

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
    database: std::path::PathBuf,
    questions: Vec<crate::app::OverviewQuestion>,
    proposals: Vec<crate::island::IslandProposal>,
    facts: WorkspaceActivityFacts,
    cached_facts: Option<WorkspaceActivityFacts>,
    area_ids: Vec<String>,
    presentation: Vec<ActivityPresentation>,
    current_area_id: String,
    files: Vec<RunOverviewFile>,
    presence: Vec<RunOverviewProjectPresence>,
}

struct ActivityOutput {
    island: crate::island::IslandWorkspace,
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
    let areas = overview_areas_from_activity(&input.area_ids, &view, &input.presentation);
    let overview =
        project_run_overview(&input.current_area_id, &view, &input.files, &input.presence);
    // 状态条与抽屉统计同源：数字都从这一份概览投影里来。
    capsule.apply_run_overview(&overview);
    let island = project::island_workspace(&input, &view);
    let summary = WorkspaceActivitySummary::from_facts("desktop", input.facts, now);
    ActivityOutput {
        island,
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
    pub(super) fn invalidate_workspace_activity(&mut self) {
        self.desktop_workspace.activity_generation =
            self.desktop_workspace.activity_generation.wrapping_add(1);
        self.desktop_workspace.activity_summary = None;
        self.desktop_workspace.activity_view = None;
        self.desktop_workspace.activity_overview = None;
        self.desktop_workspace.activity_run_overview = None;
        self.desktop_workspace.activity_capsule = None;
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

    /// The island source is the desktop host. A background session must not
    /// publish its own entity id, or locate cannot find the card.
    pub(crate) fn island_completion_route(
        &self,
        node: u64,
        cx: &Context<Self>,
    ) -> Option<crate::island::CompletionRoute> {
        let host = self.desktop_host.as_ref().and_then(|host| host.upgrade())?;
        let self_id = cx.entity().entity_id();
        let (workspace, area) = host
            .read(cx)
            .desktop_workspace
            .sessions
            .iter()
            .find(|(_, session)| session.entity_id() == self_id)
            .map(|(key, _)| key.clone())?;
        Some(crate::island::CompletionRoute {
            source: host.entity_id().as_u64(),
            workspace,
            area,
            node,
        })
    }

    pub(crate) fn note_workspace_activity_from_frame(
        &mut self,
        hook_changed: bool,
        events: &[RuntimeEvent],
        cx: &mut Context<Self>,
    ) {
        for node in events.iter().filter_map(runtime::island::completion_node) {
            let label = self
                .runtime
                .document
                .node(node)
                .and_then(|node| {
                    if let canvas::CanvasNodeKind::Agent(spec) = &node.kind {
                        Some(
                            spec.alias
                                .clone()
                                .unwrap_or_else(|| spec.profile.display_title().to_owned()),
                        )
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            crate::island::task_completed(self.island_completion_route(node, cx), label, cx);
        }
        if hook_changed || events.iter().any(workspace_activity_event) {
            self.note_workspace_activity_changed(cx);
        }
    }

    pub(super) fn mark_workspace_activity_dirty(&mut self, cx: &mut Context<Self>) {
        // Embedded projects contribute through their desktop host, never as a second source.
        if self.desktop_host.is_some() || self.desktop_workspace.activity_monitor.stopped {
            return;
        }
        if !self.desktop_workspace.activity_monitor.registered {
            self.desktop_workspace.activity_monitor.registered = true;
            let source = cx.entity().entity_id().as_u64();
            crate::island::register_source(&cx.entity(), cx);
            cx.on_release(move |_, cx| crate::island::remove_source(source, cx))
                .detach();
            self.start_island_activity_monitor(cx);
        }
        if self.desktop_workspace.activity_gate.mark() {
            self.spawn_workspace_activity_refresh(cx);
        }
    }

    pub(crate) fn note_workspace_activity_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            host.update(cx, |host, cx| host.mark_workspace_activity_dirty(cx));
            return;
        }
        if matches!(self.screen, crate::app::AppScreen::DesktopWorkspace)
            || !self.desktop_workspace.sessions.is_empty()
        {
            self.mark_workspace_activity_dirty(cx);
        }
    }

    fn ensure_proposal_snapshots(&mut self, cx: &mut Context<Self>) {
        let sessions: Vec<_> = self.desktop_workspace.sessions.values().cloned().collect();
        for session in sessions {
            session.update(cx, |app, cx| app.load_brain_proposals(cx));
        }
    }

    fn spawn_workspace_activity_refresh(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Ok(input) = this.update(cx, |app, cx| {
                app.desktop_workspace.activity_gate.begin();
                app.ensure_proposal_snapshots(cx);
                app.capture_activity_input(cx)
            }) else {
                return;
            };
            let generation = input.generation;
            let database = input.database.clone();
            let version = cx.update(crate::island::begin_refresh).unwrap_or_default();
            let projected = cx
                .background_executor()
                .spawn(async move { project::project_workspaces(input) })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.activity_generation == generation
                    && !app.desktop_workspace.activity_monitor.stopped
                {
                    match projected.islands {
                        Ok(islands) => {
                            crate::island::publish(
                                cx.entity().entity_id().as_u64(),
                                version,
                                database,
                                islands,
                                cx,
                            );
                        }
                        Err(error) => {
                            tracing::warn!(%error, "workspace activity projection failed")
                        }
                    }
                    if let Some(current) = projected.current {
                        app.apply_current_activity(current, cx);
                    }
                }
                if app.desktop_workspace.activity_gate.finish() {
                    app.spawn_workspace_activity_refresh(cx);
                }
            });
        })
        .detach();
    }

    fn apply_current_activity(&mut self, projected: ActivityOutput, cx: &mut Context<Self>) {
        let changed = self.desktop_workspace.activity_run_overview.as_ref()
            != Some(&projected.overview)
            || self.desktop_workspace.activity_overview.as_ref() != Some(&projected.areas)
            || self.desktop_workspace.activity_capsule.as_ref() != Some(&projected.capsule);
        self.desktop_workspace.activity_summary = Some(projected.summary);
        self.desktop_workspace.activity_view = Some(projected.view);
        self.desktop_workspace.activity_overview = Some(projected.areas);
        self.desktop_workspace.activity_run_overview = Some(projected.overview);
        self.desktop_workspace.activity_capsule = Some(projected.capsule);
        if changed {
            self.desktop_workspace.activity_updated_at_ms = Some(overview_clock_ms() as i64);
            cx.notify();
        }
    }
}

pub(in crate::workspace::desktop) fn overview_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}
