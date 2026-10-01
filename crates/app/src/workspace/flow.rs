//! Workspace opening flow for the app shell.

use crate::app::{AppScreen, MultisoulApp};
use crate::canvas::view::mark_canvas_open_render_pending;
use crate::motion::MotionStore;
use crate::settings::agents::AgentDefinitionsState;
use crate::workspace::persistence::PersistenceScheduler;
use crate::workspace::state::WorkspaceState;
use canvas::{CanvasNodeKind, NodeId};
use gpui::{AppContext, Context, Window};
use runtime::Runtime;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use storage::ProjectStore;
use workspace::WorkspaceSource;

#[path = "flow/session_restore_plan.rs"]
mod session_restore_plan;

mod pending_restore;
pub(crate) use pending_restore::PendingLiveSessionRestore;

#[derive(Debug)]
pub(crate) struct WorkspaceOpenResult {
    pending_live_restore: PendingLiveSessionRestore,
}

pub(crate) struct WorkspaceCanvasState<'a> {
    pub(crate) motion: &'a mut MotionStore,
    pub(crate) master_layout: &'a mut crate::canvas::master_layout::MasterLayoutState,
}

pub(crate) struct WorkspaceOpenContext<'a> {
    pub(crate) database_paths: &'a storage::DatabasePaths,
    pub(crate) runtime: &'a mut Runtime,
    pub(crate) canvas: WorkspaceCanvasState<'a>,
    pub(crate) persistence: &'a mut PersistenceScheduler,
    pub(crate) agent_definitions: &'a mut AgentDefinitionsState,
    pub(crate) screen: &'a mut AppScreen,
    pub(crate) workspace_state: &'a mut WorkspaceState,
}

impl MultisoulApp {
    pub(crate) fn open_workspace_path(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_workspace_path_with_source(path, WorkspaceSource::Open, window, cx);
    }

    pub(crate) fn open_workspace_path_with_source(
        &mut self,
        path: PathBuf,
        source: WorkspaceSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.confirm_preview_edits_before_leaving(cx) {
            return;
        }
        self.agent_runtime = None;
        self.agent_queue_ui.close();
        let result = enter_workspace_for_path_with_source(
            WorkspaceOpenContext {
                database_paths: &self.database_paths,
                runtime: &mut self.runtime,
                canvas: WorkspaceCanvasState {
                    motion: &mut self.motion,
                    master_layout: &mut self.master_layout,
                },
                persistence: &mut self.persistence,
                agent_definitions: &mut self.agent_definitions,
                screen: &mut self.screen,
                workspace_state: &mut self.workspace_state,
            },
            &path,
            source,
        );
        match result {
            Ok(opened) => {
                self.register_norma_workspace_route(&path, window, cx);
                self.terminal_render_cache.borrow_mut().clear();
                self.agent_task_coordinator = None;
                self.pending_agent_submissions.clear();
                let _ = self.session_reply_queue.interrupt_all();
                self.bind_conversation_tails_for_project();
                self.reset_agent_announcements();
                self.prune_agent_announcement_audio_cache();
                self.dialogue.stop_shell();
                self.focus_handle.focus(window);
                self.clear_browser_views();
                self.clear_ego_previews(cx);
                self.reload_all_previews(cx);
                self.spawn_deferred_live_session_restore(opened.pending_live_restore, cx);
                self.schedule_feature_guide_after_workspace_open(cx);
            }
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "failed to open workspace");
                self.workspace_state.error = Some(error);
            }
        }
        self.last_reflow_window_size = None;
        cx.notify();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn open_workspace_for_e2e(
        &mut self,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.agent_runtime = None;
        self.agent_queue_ui.close();
        let opened = enter_workspace_for_path(
            WorkspaceOpenContext {
                database_paths: &self.database_paths,
                runtime: &mut self.runtime,
                canvas: WorkspaceCanvasState {
                    motion: &mut self.motion,
                    master_layout: &mut self.master_layout,
                },
                persistence: &mut self.persistence,
                agent_definitions: &mut self.agent_definitions,
                screen: &mut self.screen,
                workspace_state: &mut self.workspace_state,
            },
            path,
        )?;
        self.terminal_render_cache.borrow_mut().clear();
        self.agent_task_coordinator = None;
        self.pending_agent_submissions.clear();
        let _ = self.session_reply_queue.interrupt_all();
        self.bind_conversation_tails_for_project();
        self.reset_agent_announcements();
        self.prune_agent_announcement_audio_cache();
        self.dialogue.stop_shell();
        self.clear_browser_views();
        self.clear_ego_previews(cx);
        self.reload_all_previews(cx);
        self.spawn_deferred_live_session_restore(opened.pending_live_restore, cx);
        self.schedule_feature_guide_after_workspace_open(cx);
        self.last_reflow_window_size = None;
        cx.notify();
        Ok(())
    }

    pub(crate) fn spawn_deferred_live_session_restore(
        &mut self,
        pending: PendingLiveSessionRestore,
        cx: &mut Context<Self>,
    ) {
        if pending.is_empty() {
            self.background_restore = None;
            return;
        }
        self.background_restore = Some(pending.clone());
        let source_window = self.project_window;
        // Desktop areas own live sessions in overview too; window attachment only changes presentation.
        let desktop_restore = self.desktop_host.is_some();
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
                    let _ = this.update(cx, |app, _| {
                        if (!app.background_project || desktop_restore)
                            && (desktop_restore || app.project_window == source_window)
                            && app.workspace_state.current_root.as_deref() == Some(root.as_path())
                        {
                            app.background_restore = None;
                        }
                    });
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
                let discovery_profile = this.update(cx, |app, _cx| {
                    if (!desktop_restore
                        && (app.background_project || app.project_window != source_window))
                        || app.workspace_state.current_root.as_deref() != Some(root.as_path())
                    {
                        return None;
                    }
                    if app.runtime.node_run_state(node_id).is_some()
                        || !pending.legacy_discovery_node_ids.contains(&node_id)
                    {
                        return None;
                    }
                    match app.runtime.document.node(node_id).map(|node| &node.kind) {
                        Some(CanvasNodeKind::Agent(spec)) => Some(spec.profile),
                        _ => None,
                    }
                });
                let discovery_profile = match discovery_profile {
                    Ok(profile) => profile,
                    Err(_) => break,
                };
                let discovered_session_id = match discovery_profile {
                    Some(profile) => {
                        let discovery_root = root.clone();
                        cx.background_spawn(async move {
                            runtime::session_snapshot::find_agent_session_id_for_profile(
                                profile,
                                &discovery_root,
                            )
                        })
                        .await
                    }
                    None => None,
                };
                let update = this.update(cx, |app, cx| {
                    if (!desktop_restore
                        && (app.background_project || app.project_window != source_window))
                        || app.workspace_state.current_root.as_deref() != Some(root.as_path())
                    {
                        return false;
                    }
                    if app.runtime.node_run_state(node_id).is_some() {
                        return true;
                    }
                    let restore_start = Instant::now();
                    let binding_changed =
                        discovered_session_id.as_deref().is_some_and(|session_id| {
                            app.runtime
                                .bind_discovered_agent_session(node_id, session_id)
                        });
                    if binding_changed {
                        app.mark_session_dirty();
                    }
                    let restored = app
                        .runtime
                        .restore_live_session_for_node_with_node_sessions(
                            node_id,
                            &pending.checkpoints,
                            &pending.node_sessions,
                            None,
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

#[cfg(any(test, feature = "e2e-test-support"))]
pub(crate) fn enter_workspace_for_path(
    context: WorkspaceOpenContext<'_>,
    path: &Path,
) -> Result<WorkspaceOpenResult, String> {
    enter_workspace_for_path_with_source(context, path, WorkspaceSource::Open)
}

pub(crate) fn enter_workspace_for_path_with_source(
    context: WorkspaceOpenContext<'_>,
    path: &Path,
    source: WorkspaceSource,
) -> Result<WorkspaceOpenResult, String> {
    let WorkspaceOpenContext {
        database_paths,
        runtime,
        canvas,
        persistence,
        agent_definitions,
        screen,
        workspace_state,
    } = context;
    let WorkspaceCanvasState {
        motion,
        master_layout,
    } = canvas;
    let total_start = Instant::now();
    let mut stage_start = total_start;
    let previous_root = workspace_state.current_root.clone();
    let root = workspace_state.open_workspace(path, source)?;
    // The selected directory is not an open project until restoration succeeds.
    workspace_state.current_root = previous_root;
    log_workspace_open_timing(
        "workspace_open_record_recent",
        &root,
        &mut stage_start,
        total_start,
    );
    let project_store = ProjectStore::initialize_for_database_paths(database_paths, &root)
        .map_err(|error| format!("Could not initialize project storage: {error}"))?;
    log_workspace_open_timing(
        "workspace_open_project_store",
        &root,
        &mut stage_start,
        total_start,
    );
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
    crate::workspace::legacy_agent_migration::migrate_legacy_agent_cards(
        &mut next_runtime,
        &project_store,
    )?;
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
    let hydration = next_runtime.hydrate_agent_session_bindings(&node_sessions);
    if !hydration.conflicting_node_ids.is_empty() {
        tracing::warn!(
            conflicting_node_ids = ?hydration.conflicting_node_ids,
            "duplicate Agent session bindings quarantined during workspace open"
        );
    }
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
    persistence.retained_checkpoints = pending_live_restore.checkpoints.clone();
    agent_definitions.bind_project_store(agent_definition_store);
    motion.clear_transient();
    motion.retain_nodes(std::iter::empty::<NodeId>());
    start_restored_card_initializations(runtime, motion);
    *screen = AppScreen::Canvas;
    workspace_state.current_root = Some(root.clone());
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

/// 恢复 workspace 时，卡片先以静态 document snapshot 呈现；PTY session 仍由后续任务
/// 分批恢复。这里仅登记 UI 覆盖层，不做任何 session IO，因此不会延迟 Canvas 首帧。
fn start_restored_card_initializations(runtime: &Runtime, motion: &mut MotionStore) {
    for node in runtime.document.nodes() {
        let kind = match &node.kind {
            CanvasNodeKind::Term(_) => Some(crate::motion::InitializationAnimationKind::Terminal),
            CanvasNodeKind::Agent(spec) => {
                Some(crate::motion::InitializationAnimationKind::for_agent_profile(spec.profile))
            }
            CanvasNodeKind::Rect(_) | CanvasNodeKind::Preview(_) | CanvasNodeKind::Browser(_) => {
                None
            }
        };
        if let Some(kind) = kind {
            motion.ensure_node(node.id, node.bounds);
            motion.start_initialization(node.id, kind);
        }
    }
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

#[cfg(test)]
#[path = "../../tests/unit/workspace_deferred_restore_tests.rs"]
mod deferred_restore_tests;
