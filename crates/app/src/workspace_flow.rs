//! Workspace opening flow for the app shell.

use crate::app::{AppScreen, MultisoulApp};
use crate::motion::MotionStore;
use crate::persistence::PersistenceScheduler;
use crate::workspace_state::WorkspaceState;
use canvas::NodeId;
use gpui::{Context, PathPromptOptions};
use runtime::Runtime;
use std::path::{Path, PathBuf};
use storage::ProjectStore;
use workspace::WorkspaceSource;

impl MultisoulApp {
    pub(crate) fn prompt_open_project(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Project".into()),
        });

        cx.spawn(async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.update(&mut *cx, |app, cx| {
                            app.open_workspace_path(path, cx);
                        })?;
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error =
                            Some(format!("Could not open project picker: {error}"));
                        cx.notify();
                    })?;
                }
                Err(error) => {
                    this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error = Some(format!(
                            "Project picker was cancelled unexpectedly: {error}"
                        ));
                        cx.notify();
                    })?;
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    pub(crate) fn open_workspace_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let result = enter_workspace_for_path(
            &mut self.runtime,
            &mut self.motion,
            &mut self.master_layout,
            &mut self.persistence,
            &mut self.screen,
            &mut self.workspace_state,
            &path,
        );
        match result {
            Ok(()) => self.reload_all_previews(cx),
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
    master_layout: &mut crate::master_layout::MasterLayoutState,
    persistence: &mut PersistenceScheduler,
    screen: &mut AppScreen,
    workspace_state: &mut WorkspaceState,
    path: &Path,
) -> Result<(), String> {
    let root = workspace_state.open_workspace(path, WorkspaceSource::Open)?;
    let project_store = ProjectStore::for_workspace(&root)
        .map_err(|error| format!("Could not initialize project storage: {error}"))?;
    project_store
        .migrate()
        .map_err(|error| format!("Could not migrate project storage: {error}"))?;
    let mut next_runtime = Runtime::new_for_workspace(root);
    next_runtime
        .load_canvas_snapshot(&project_store.canvas_store())
        .map_err(|error| format!("Could not restore canvas snapshot: {error}"))?;
    let checkpoints = project_store
        .session_store()
        .load_checkpoints()
        .map_err(|error| format!("Could not load session checkpoints: {error}"))?;
    let restored_sessions = next_runtime.restore_live_sessions(&checkpoints);
    tracing::info!(
        restored_sessions,
        "restored live sessions for opened workspace"
    );
    runtime.kill_all();
    *runtime = next_runtime;
    persistence.set_project_store(project_store);
    master_layout.clear();
    motion.clear_transient();
    motion.retain_nodes(std::iter::empty::<NodeId>());
    *screen = AppScreen::Canvas;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master_layout::MasterLayoutState;
    use canvas::{
        CanvasNodeKind, CanvasNodeSnapshot, CanvasRect, DocumentSnapshot, TermSpec, Vec2, Viewport,
    };
    use runtime::NodeView;
    use storage::{RecentStore, SessionCheckpoint, SessionCheckpointKind};

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
        let mut screen = AppScreen::Launchpad;

        let result = enter_workspace_for_path(
            &mut runtime,
            &mut motion,
            &mut master_layout,
            &mut persistence,
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
}
