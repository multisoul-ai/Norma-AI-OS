//! Workspace opening flow for the app shell.

use crate::app::{AppScreen, MultisoulApp};
use crate::canvas::view::mark_canvas_open_render_pending;
use crate::motion::MotionStore;
use crate::settings::agents::AgentDefinitionsState;
use crate::workspace::persistence::PersistenceScheduler;
use crate::workspace::state::WorkspaceState;
use canvas::NodeId;
use gpui::{Context, PathPromptOptions, Window};
use runtime::Runtime;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use storage::{NodeSession, ProjectStore, SessionCheckpoint};
use workspace::WorkspaceSource;

#[derive(Clone, Debug)]
pub(crate) struct PendingLiveSessionRestore {
    root: PathBuf,
    checkpoints: Vec<SessionCheckpoint>,
    node_sessions: Vec<NodeSession>,
    node_ids: Vec<NodeId>,
}

impl PendingLiveSessionRestore {
    fn new(
        root: PathBuf,
        runtime: &Runtime,
        checkpoints: Vec<SessionCheckpoint>,
        node_sessions: Vec<NodeSession>,
    ) -> Self {
        let node_ids = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    canvas::CanvasNodeKind::Term(_) | canvas::CanvasNodeKind::Agent(_)
                )
            })
            .map(|node| node.id)
            .collect();
        Self {
            root,
            checkpoints,
            node_sessions,
            node_ids,
        }
    }

    fn len(&self) -> usize {
        self.node_ids.len()
    }

    fn is_empty(&self) -> bool {
        self.node_ids.is_empty()
    }
}

#[derive(Debug)]
pub(crate) struct WorkspaceOpenResult {
    pending_live_restore: PendingLiveSessionRestore,
}

impl MultisoulApp {
    pub(crate) fn prompt_open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Project".into()),
        });
        let current_window = window.window_handle().downcast::<MultisoulApp>();

        cx.spawn(async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        if let Some(current_window) = current_window {
                            let _ = current_window.update(&mut *cx, |app, window, cx| {
                                app.open_workspace_in_project_window(path, true, window, cx);
                            });
                        }
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    let _ = this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error =
                            Some(format!("Could not open project picker: {error}"));
                        cx.notify();
                    });
                }
                Err(error) => {
                    let _ = this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error = Some(format!(
                            "Project picker was cancelled unexpectedly: {error}"
                        ));
                        cx.notify();
                    });
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    pub(crate) fn open_workspace_path(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = enter_workspace_for_path(
            &mut self.runtime,
            &mut self.motion,
            &mut self.master_layout,
            &mut self.persistence,
            &mut self.agent_definitions,
            &mut self.screen,
            &mut self.workspace_state,
            &path,
        );
        match result {
            Ok(opened) => {
                self.focus_handle.focus(window);
                self.reload_all_previews(cx);
                self.spawn_deferred_live_session_restore(opened.pending_live_restore, cx);
            }
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "failed to open workspace");
            }
        }
        self.last_reflow_window_size = None;
        cx.notify();
    }

    fn spawn_deferred_live_session_restore(
        &mut self,
        pending: PendingLiveSessionRestore,
        cx: &mut Context<Self>,
    ) {
        if pending.is_empty() {
            return;
        }

        let root = pending.root.clone();
        let total_sessions = pending.len();
        tracing::info!(
            stage = "workspace_open_restore_live_sessions_deferred_start",
            path = %root.display(),
            total_sessions,
            "workspace_open_timing"
        );
        cx.spawn(async move |this, cx| {
            let mut next_index = 0usize;
            loop {
                if next_index >= pending.node_ids.len() {
                    tracing::info!(
                        stage = "workspace_open_restore_live_sessions_deferred_done",
                        path = %root.display(),
                        total_sessions,
                        "workspace_open_timing"
                    );
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let node_id = pending.node_ids[next_index];
                next_index += 1;
                let remaining = pending.node_ids.len().saturating_sub(next_index);
                let update = this.update(cx, |app, cx| {
                    if app.workspace_state.current_root.as_deref() != Some(root.as_path()) {
                        return false;
                    }
                    let restore_start = Instant::now();
                    let restored = app
                        .runtime
                        .restore_live_session_for_node_with_node_sessions(
                            node_id,
                            &pending.checkpoints,
                            &pending.node_sessions,
                        );
                    tracing::info!(
                        stage = "workspace_open_restore_live_session_node",
                        path = %root.display(),
                        node_id,
                        restored,
                        remaining,
                        elapsed_ms = restore_start.elapsed().as_secs_f64() * 1000.0,
                        "workspace_open_timing"
                    );
                    if restored {
                        cx.notify();
                    }
                    true
                });
                match update {
                    Ok(true) => {}
                    Ok(false) | Err(_) => break,
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }
}

pub(crate) fn enter_workspace_for_path(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    persistence: &mut PersistenceScheduler,
    agent_definitions: &mut AgentDefinitionsState,
    screen: &mut AppScreen,
    workspace_state: &mut WorkspaceState,
    path: &Path,
) -> Result<WorkspaceOpenResult, String> {
    let total_start = Instant::now();
    let mut stage_start = total_start;
    let root = workspace_state.open_workspace(path, WorkspaceSource::Open)?;
    log_workspace_open_timing(
        "workspace_open_record_recent",
        &root,
        &mut stage_start,
        total_start,
    );
    let project_store = ProjectStore::for_workspace(&root)
        .map_err(|error| format!("Could not initialize project storage: {error}"))?;
    log_workspace_open_timing(
        "workspace_open_project_store",
        &root,
        &mut stage_start,
        total_start,
    );
    project_store
        .migrate()
        .map_err(|error| format!("Could not migrate project storage: {error}"))?;
    log_workspace_open_timing(
        "workspace_open_migrate",
        &root,
        &mut stage_start,
        total_start,
    );
    // Flush cwd, canvas, and layout state before replacing runtime/master layout
    // with the newly opened workspace.
    let _ = persistence.flush_now(runtime, master_layout);
    log_workspace_open_timing(
        "workspace_open_flush_previous",
        &root,
        &mut stage_start,
        total_start,
    );
    let hook_config = runtime.agent_hook_config().cloned();
    let mut next_runtime = Runtime::new_for_workspace(root.clone());
    next_runtime.set_agent_hook_config(hook_config);
    next_runtime
        .load_canvas_snapshot(&project_store.canvas_store())
        .map_err(|error| format!("Could not restore canvas snapshot: {error}"))?;
    log_workspace_open_timing(
        "workspace_open_load_canvas_snapshot",
        &root,
        &mut stage_start,
        total_start,
    );
    match project_store.layout_store().load_state() {
        Ok(Some(layout_state)) => {
            master_layout.restore_from_snapshot(layout_state, &next_runtime);
        }
        Ok(None) => master_layout.clear(),
        Err(error) => return Err(format!("Could not restore layout state: {error}")),
    }
    log_workspace_open_timing(
        "workspace_open_restore_layout",
        &root,
        &mut stage_start,
        total_start,
    );
    let checkpoints = project_store
        .session_store()
        .load_checkpoints()
        .map_err(|error| format!("Could not load session checkpoints: {error}"))?;
    log_workspace_open_timing(
        "workspace_open_load_checkpoints",
        &root,
        &mut stage_start,
        total_start,
    );
    let node_sessions = project_store
        .node_session_store()
        .load_sessions()
        .unwrap_or_default();
    log_workspace_open_timing(
        "workspace_open_load_node_sessions",
        &root,
        &mut stage_start,
        total_start,
    );
    tracing::info!(
        stage = "workspace_open_restore_live_sessions",
        path = %root.display(),
        checkpoints = checkpoints.len(),
        node_sessions = node_sessions.len(),
        pending_sessions = next_runtime.document.nodes().iter().filter(|node| {
            matches!(
                node.kind,
                canvas::CanvasNodeKind::Term(_) | canvas::CanvasNodeKind::Agent(_)
            )
        }).count(),
        stage_ms = stage_start.elapsed().as_secs_f64() * 1000.0,
        total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
        "workspace_open_timing"
    );
    stage_start = Instant::now();
    let pending_live_restore =
        PendingLiveSessionRestore::new(root.clone(), &next_runtime, checkpoints, node_sessions);
    let agent_definition_store = project_store.agent_definition_store();
    runtime.kill_all();
    *runtime = next_runtime;
    persistence.set_project_store(project_store);
    agent_definitions.bind_project_store(agent_definition_store);
    motion.clear_transient();
    motion.retain_nodes(std::iter::empty::<NodeId>());
    *screen = AppScreen::Canvas;
    mark_canvas_open_render_pending();
    tracing::info!(
        stage = "workspace_open_total",
        path = %root.display(),
        total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
        final_swap_ms = stage_start.elapsed().as_secs_f64() * 1000.0,
        "workspace_open_timing"
    );
    Ok(WorkspaceOpenResult {
        pending_live_restore,
    })
}

fn log_workspace_open_timing(
    stage: &'static str,
    root: &Path,
    stage_start: &mut Instant,
    total_start: Instant,
) {
    tracing::info!(
        stage,
        path = %root.display(),
        stage_ms = stage_start.elapsed().as_secs_f64() * 1000.0,
        total_ms = total_start.elapsed().as_secs_f64() * 1000.0,
        "workspace_open_timing"
    );
    *stage_start = Instant::now();
}

#[cfg(test)]
#[path = "../../tests/unit/workspace_flow_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/workspace_open_timing_tests.rs"]
mod workspace_open_timing_tests;
