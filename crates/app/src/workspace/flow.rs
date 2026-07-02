//! Workspace opening flow for the app shell.

use crate::app::{AppScreen, MultisoulApp};
use crate::motion::MotionStore;
use crate::settings::agents::AgentDefinitionsState;
use crate::workspace::persistence::PersistenceScheduler;
use crate::workspace::state::WorkspaceState;
use canvas::NodeId;
use gpui::{Context, PathPromptOptions, Window};
use runtime::Runtime;
use std::path::{Path, PathBuf};
use storage::ProjectStore;
use workspace::WorkspaceSource;

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
            Ok(_root) => {
                self.focus_handle.focus(window);
                self.reload_all_previews(cx);
            }
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "failed to open workspace");
            }
        }
        self.last_reflow_window_size = None;
        cx.notify();
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
) -> Result<PathBuf, String> {
    let root = workspace_state.open_workspace(path, WorkspaceSource::Open)?;
    let project_store = ProjectStore::for_workspace(&root)
        .map_err(|error| format!("Could not initialize project storage: {error}"))?;
    project_store
        .migrate()
        .map_err(|error| format!("Could not migrate project storage: {error}"))?;
    // Flush cwd, canvas, and layout state before replacing runtime/master layout
    // with the newly opened workspace.
    let _ = persistence.flush_now(runtime, master_layout);
    let hook_config = runtime.agent_hook_config().cloned();
    let mut next_runtime = Runtime::new_for_workspace(root.clone());
    next_runtime.set_agent_hook_config(hook_config);
    next_runtime
        .load_canvas_snapshot(&project_store.canvas_store())
        .map_err(|error| format!("Could not restore canvas snapshot: {error}"))?;
    match project_store.layout_store().load_state() {
        Ok(Some(layout_state)) => {
            master_layout.restore_from_snapshot(layout_state, &next_runtime);
        }
        Ok(None) => master_layout.clear(),
        Err(error) => return Err(format!("Could not restore layout state: {error}")),
    }
    let checkpoints = project_store
        .session_store()
        .load_checkpoints()
        .map_err(|error| format!("Could not load session checkpoints: {error}"))?;
    let node_sessions = project_store
        .node_session_store()
        .load_sessions()
        .unwrap_or_default();
    let restored_sessions =
        next_runtime.restore_live_sessions_with_node_sessions(&checkpoints, &node_sessions);
    tracing::info!(
        restored_sessions,
        "restored live sessions for opened workspace"
    );
    let agent_definition_store = project_store.agent_definition_store();
    runtime.kill_all();
    *runtime = next_runtime;
    persistence.set_project_store(project_store);
    agent_definitions.bind_project_store(agent_definition_store);
    motion.clear_transient();
    motion.retain_nodes(std::iter::empty::<NodeId>());
    *screen = AppScreen::Canvas;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::master_layout::MasterLayoutState;
    use canvas::{
        CanvasNodeKind, CanvasNodeSnapshot, CanvasRect, DocumentSnapshot, TermSpec, Vec2, Viewport,
    };
    use runtime::NodeView;
    use storage::{
        LayoutMode, LayoutOriginalBounds, LayoutStateSnapshot, RecentStore, SessionCheckpoint,
        SessionCheckpointKind,
    };

    /// 打开已有 workspace：app 层应恢复布局后重新挂载 live terminal，避免卡片显示 session gone。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   workspace path = temp/restored-live-terminal，真实目录
    ///   canvas snapshot = [Term id=6,bounds(10,20,640,360)]
    ///   session checkpoint = node_id=6，shell="/bin/sh"，cols=88，rows=26
    ///   初始 runtime.live_len = 0，打开后应恢复为 1
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. ProjectStore 写入 canvas snapshot 与 session checkpoint
    ///   2. enter_workspace_for_path(workspace)
    ///      → load_canvas_snapshot 恢复 document
    ///      → load_checkpoints + restore_live_sessions 为 id=6 重新 spawn live terminal
    ///   3. 查询 runtime.node_view(6)
    ///
    /// 预期结果：
    ///   - 正断言：打开成功，screen == Canvas，live_len == 1
    ///   - 正断言：node_view(6) 是 Term 且尺寸为 88x26
    ///   - 负断言：node_view(6) 不应为 None，否则 UI 会显示 session gone
    #[test]
    fn test_enter_workspace_restores_live_terminal_for_saved_card() {
        let temp = tempfile::tempdir().expect("test should create temp dir");
        let workspace_path = temp.path().join("restored-live-terminal");
        std::fs::create_dir(&workspace_path).expect("test should create workspace dir");
        let project_store =
            ProjectStore::for_workspace(&workspace_path).expect("project store should open");
        let snapshot = DocumentSnapshot {
            nodes: vec![CanvasNodeSnapshot {
                id: 6,
                kind: CanvasNodeKind::Term(TermSpec),
                bounds: CanvasRect::new(10.0, 20.0, 640.0, 360.0),
                z_index: 2,
            }],
            viewport: Viewport {
                pan: Vec2::new(4.0, 5.0),
                zoom: 1.25,
            },
        };
        let checkpoint = SessionCheckpoint {
            node_id: 6,
            kind: SessionCheckpointKind::Terminal,
            payload: "Terminal".into(),
            cols: 88,
            rows: 26,
            shell: Some("/bin/sh".into()),
            agent_profile: None,
        };
        project_store
            .canvas_store()
            .save_snapshot(&snapshot)
            .expect("canvas snapshot 应写入 project store");
        project_store
            .session_store()
            .save_checkpoint(&checkpoint)
            .expect("session checkpoint 应写入 project store");
        let storage = RecentStore::new(temp.path().join("state").join("app.sqlite"));
        let mut workspace_state = WorkspaceState::load_from_storage(storage);
        let mut runtime = Runtime::new();
        let mut motion = MotionStore::new(true);
        let mut master_layout = MasterLayoutState::default();
        let mut persistence = PersistenceScheduler::default();
        let mut agent_definitions = AgentDefinitionsState::default();
        let mut screen = AppScreen::Launchpad;

        let result = enter_workspace_for_path(
            &mut runtime,
            &mut motion,
            &mut master_layout,
            &mut persistence,
            &mut agent_definitions,
            &mut screen,
            &mut workspace_state,
            &workspace_path,
        );
        let view = runtime.node_view(6);

        assert!(
            result.is_ok(),
            "enter_workspace_for_path 应成功打开已持久化的 workspace，实际: {:?}",
            result
        );
        assert_eq!(
            screen,
            AppScreen::Canvas,
            "成功打开 workspace 后应切换到 Canvas"
        );
        assert_eq!(
            runtime.live_len(),
            1,
            "打开 workspace 后应为保存的 terminal card 恢复 1 个 live node"
        );
        let Some(NodeView::Term { snapshot, .. }) = view else {
            runtime.kill_all();
            panic!("node_view(6) 不应为 None，否则终端卡片会显示 session gone");
        };
        assert_eq!(
            snapshot.cols, 88,
            "恢复的 live terminal 应使用 checkpoint 中的 88 列"
        );
        assert_eq!(
            snapshot.lines, 26,
            "恢复的 live terminal 应使用 checkpoint 中的 26 行"
        );
        assert!(
            runtime.node_view(6).is_some(),
            "保存过 checkpoint 的 terminal card 不应进入 session gone 状态"
        );
        assert!(
            runtime.node_view(7).is_none(),
            "恢复 node_id=6 不应额外创建 node_id=7 的 live node"
        );
        runtime.kill_all();
    }

    /// 打开已有 workspace：app 层应恢复保存的 master/detail layout state。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   canvas snapshot = Rect A(id=1,bounds 10/20/100/80), Rect B(id=2,bounds 140/20/100/80)
    ///   layout_state = MasterDetail(master_id=2)，original_bounds 记录 A/B 进入 master 前 bounds
    ///   初始 master_layout inactive，打开后应 active 且 master_id=2
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. ProjectStore 写入 canvas snapshot 与 layout_state
    ///   2. enter_workspace_for_path(workspace)
    ///      → load_canvas_snapshot 恢复 document
    ///      → load_state + restore_from_snapshot 恢复 MasterLayoutState
    ///   3. 查询 master_layout.master_id()
    ///
    /// 预期结果：
    ///   - 正断言：打开成功，screen == Canvas，master_layout active master 为 2
    ///   - 正断言：runtime document 仍包含 A/B
    ///   - 负断言：master_layout 不应被 workspace open 末尾 clear 掉
    #[test]
    fn test_enter_workspace_restores_saved_master_layout_state() {
        let temp = tempfile::tempdir().expect("test should create temp dir");
        let workspace_path = temp.path().join("restored-master-layout");
        std::fs::create_dir(&workspace_path).expect("test should create workspace dir");
        let project_store =
            ProjectStore::for_workspace(&workspace_path).expect("project store should open");
        let a_bounds = CanvasRect::new(10.0, 20.0, 100.0, 80.0);
        let b_bounds = CanvasRect::new(140.0, 20.0, 100.0, 80.0);
        let snapshot = DocumentSnapshot {
            nodes: vec![
                CanvasNodeSnapshot {
                    id: 1,
                    kind: CanvasNodeKind::Rect(canvas::RectNode { color: 0x4080ff }),
                    bounds: a_bounds,
                    z_index: 1,
                },
                CanvasNodeSnapshot {
                    id: 2,
                    kind: CanvasNodeKind::Rect(canvas::RectNode { color: 0xff8040 }),
                    bounds: b_bounds,
                    z_index: 2,
                },
            ],
            viewport: Viewport {
                pan: Vec2::new(0.0, 0.0),
                zoom: 1.0,
            },
        };
        project_store
            .canvas_store()
            .save_snapshot(&snapshot)
            .expect("canvas snapshot 应写入 project store");
        project_store
            .layout_store()
            .save_state(&LayoutStateSnapshot {
                mode: LayoutMode::MasterDetail,
                master_id: Some(2),
                original_bounds: vec![
                    LayoutOriginalBounds {
                        node_id: 1,
                        bounds: a_bounds,
                    },
                    LayoutOriginalBounds {
                        node_id: 2,
                        bounds: b_bounds,
                    },
                ],
            })
            .expect("layout state 应写入 project store");
        let storage = RecentStore::new(temp.path().join("state").join("app.sqlite"));
        let mut workspace_state = WorkspaceState::load_from_storage(storage);
        let mut runtime = Runtime::new();
        let mut motion = MotionStore::new(true);
        let mut master_layout = MasterLayoutState::default();
        let mut persistence = PersistenceScheduler::default();
        let mut agent_definitions = AgentDefinitionsState::default();
        let mut screen = AppScreen::Launchpad;

        let result = enter_workspace_for_path(
            &mut runtime,
            &mut motion,
            &mut master_layout,
            &mut persistence,
            &mut agent_definitions,
            &mut screen,
            &mut workspace_state,
            &workspace_path,
        );

        assert!(
            result.is_ok(),
            "enter_workspace_for_path 应成功打开带 layout_state 的 workspace，实际: {:?}",
            result
        );
        assert_eq!(
            screen,
            AppScreen::Canvas,
            "成功打开 workspace 后应切换到 Canvas"
        );
        assert_eq!(
            master_layout.master_id(),
            Some(2),
            "打开后应恢复 active master_id=2"
        );
        assert!(
            master_layout.is_active(),
            "打开后 master_layout 不应被 clear，应保持 active master/detail"
        );
        assert!(
            runtime.document.node(1).is_some(),
            "恢复 layout state 不应丢失 Rect A"
        );
        assert!(
            runtime.document.node(2).is_some(),
            "恢复 layout state 不应丢失 Rect B"
        );
        assert_ne!(
            master_layout.master_id(),
            None,
            "workspace open 末尾不应无条件清空 master_layout"
        );
    }
}
