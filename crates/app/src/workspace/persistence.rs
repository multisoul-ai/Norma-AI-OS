use crate::app::MultisoulApp;
use crate::canvas::master_layout::MasterLayoutState;
use runtime::Runtime;
use std::time::Duration;
use storage::VoiceHistoryEntry;
use storage::{ProjectStore, StorageError};

const PERSISTENCE_DEBOUNCE: Duration = Duration::from_millis(360);

#[derive(Debug, Default)]
pub(crate) struct PersistenceScheduler {
    project_store: Option<ProjectStore>,
    canvas_dirty: bool,
    session_dirty: bool,
    dirty_for: Duration,
}

impl PersistenceScheduler {
    pub(crate) fn set_project_store(&mut self, store: ProjectStore) {
        self.project_store = Some(store);
        self.canvas_dirty = false;
        self.session_dirty = false;
        self.dirty_for = Duration::ZERO;
    }

    pub(crate) fn mark_canvas_dirty(&mut self) {
        self.canvas_dirty = true;
        self.dirty_for = Duration::ZERO;
    }

    pub(crate) fn mark_session_dirty(&mut self) {
        self.session_dirty = true;
        self.dirty_for = Duration::ZERO;
    }

    pub(crate) fn tick(
        &mut self,
        delta: Duration,
        runtime: &Runtime,
        master_layout: &MasterLayoutState,
    ) -> Result<bool, StorageError> {
        if !self.has_pending_work() {
            return Ok(false);
        }

        self.dirty_for = self.dirty_for.saturating_add(delta);
        if self.dirty_for < PERSISTENCE_DEBOUNCE {
            return Ok(false);
        }

        self.flush_now(runtime, master_layout)
    }

    pub(crate) fn flush_now(
        &mut self,
        runtime: &Runtime,
        master_layout: &MasterLayoutState,
    ) -> Result<bool, StorageError> {
        let Some(project_store) = self.project_store.as_ref() else {
            self.canvas_dirty = false;
            self.session_dirty = false;
            self.dirty_for = Duration::ZERO;
            return Ok(false);
        };

        let had_pending_work = self.has_pending_work();
        if !had_pending_work {
            self.dirty_for = Duration::ZERO;
            return Ok(false);
        }

        if self.canvas_dirty {
            runtime.save_canvas_snapshot(&project_store.canvas_store())?;
            project_store
                .layout_store()
                .save_state(&master_layout.to_snapshot())?;
        }
        if self.session_dirty {
            runtime.save_session_checkpoints(&project_store.session_store())?;
        }
        // Node sessions are derived from live nodes and may require process metadata.
        // Keep them on dirty flushes only so close/no-op flushes stay off the SQLite path.
        runtime.save_node_sessions(&project_store.node_session_store())?;
        self.canvas_dirty = false;
        self.session_dirty = false;
        self.dirty_for = Duration::ZERO;
        Ok(had_pending_work)
    }

    fn has_pending_work(&self) -> bool {
        self.canvas_dirty || self.session_dirty
    }

    pub(crate) fn save_voice_history(
        &self,
        entry: &VoiceHistoryEntry,
    ) -> Result<Option<i64>, StorageError> {
        let Some(project_store) = self.project_store.as_ref() else {
            return Ok(None);
        };
        project_store
            .voice_history_store()
            .save_entry(entry)
            .map(Some)
    }
}

impl MultisoulApp {
    pub(crate) fn mark_canvas_dirty(&mut self) {
        self.persistence.mark_canvas_dirty();
    }

    pub(crate) fn mark_session_dirty(&mut self) {
        self.persistence.mark_session_dirty();
    }

    pub(crate) fn tick_persistence(&mut self, delta: Duration) -> bool {
        match self
            .persistence
            .tick(delta, &self.runtime, &self.master_layout)
        {
            Ok(changed) => changed,
            Err(error) => {
                tracing::warn!(%error, "workspace persistence tick failed");
                self.workspace_state.error = Some(format!("Could not save workspace: {error}"));
                true
            }
        }
    }

    pub(crate) fn flush_persistence_now(&mut self) {
        if let Err(error) = self
            .persistence
            .flush_now(&self.runtime, &self.master_layout)
        {
            tracing::warn!(%error, "workspace persistence flush failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::{CanvasNodeKind, CanvasRect, RectNode};
    use storage::{NodeSession, NodeSessionKind, ProjectStore};

    /// P-1: scheduler debounce 期间不写 SQLite，超过窗口后合并写一次 canvas snapshot。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   debounce = 360ms
    ///   first tick = 12ms，12 < 360，因此不应 flush
    ///   second tick = 360ms，累计 372 >= 360，因此应 flush
    ///   runtime document 添加 Rect 节点，node_id=1
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. set_project_store(project.sqlite)
    ///   2. mark_canvas_dirty()
    ///   3. tick(12ms) → dirty 未达到 debounce，load_snapshot 应为 None
    ///   4. tick(360ms) → flush_now 写入 canvas snapshot
    ///
    /// 预期结果：
    ///   - 正断言：第一次 tick 返回 false 且没有 snapshot
    ///   - 正断言：第二次 tick 返回 true 且 snapshot 包含 node_id=1
    ///   - 负断言：debounce 未到时不应提前写库
    #[test]
    fn test_scheduler_debounces_canvas_snapshot_flush() {
        let temp = tempfile::tempdir().expect("test should create temp dir");
        let workspace_root = temp.path().join("workspace");
        std::fs::create_dir(&workspace_root).expect("test should create workspace dir");
        let project_store =
            ProjectStore::for_workspace(&workspace_root).expect("project store should open");
        let canvas_store = project_store.canvas_store();
        let layout_store = project_store.layout_store();
        let mut scheduler = PersistenceScheduler::default();
        let mut runtime = Runtime::new();
        let node_id = runtime.document.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
            CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        );

        scheduler.set_project_store(project_store);
        scheduler.mark_canvas_dirty();
        let early = scheduler
            .tick(
                Duration::from_millis(12),
                &runtime,
                &crate::canvas::master_layout::MasterLayoutState::default(),
            )
            .expect("early tick 应成功但不 flush");
        let before = canvas_store
            .load_snapshot()
            .expect("debounce 前 snapshot 查询应成功");
        let late = scheduler
            .tick(
                Duration::from_millis(360),
                &runtime,
                &crate::canvas::master_layout::MasterLayoutState::default(),
            )
            .expect("late tick 应触发 flush");
        let after = canvas_store
            .load_snapshot()
            .expect("flush 后 snapshot 查询应成功")
            .expect("flush 后 snapshot 应存在");
        let layout_after = layout_store
            .load_state()
            .expect("flush 后 layout state 查询应成功")
            .expect("flush 后 layout state 应存在");

        assert!(!early, "12ms < 360ms 时不应 flush 持久化");
        assert!(before.is_none(), "debounce 未到时不应提前写 SQLite");
        assert!(late, "累计超过 debounce 后应 flush 持久化");
        assert_eq!(
            after.nodes[0].id, node_id,
            "flush 后 canvas snapshot 应包含 runtime 中的 node_id"
        );
        assert_ne!(
            before,
            Some(after),
            "debounce 前后不应相同，否则说明没有验证到延迟写入"
        );
        assert_eq!(
            layout_after.mode,
            storage::LayoutMode::Tiled,
            "canvas dirty flush 应同步保存默认 Tiled layout state"
        );
        assert!(
            layout_after.master_id.is_none(),
            "默认 Tiled layout state 不应保存 master_id"
        );
    }

    /// P-2: 没有脏数据时 flush_now 应快速返回，不能重写 node_sessions 表。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   stale node session = node_id=99, cwd="/tmp/stale"
    ///   canvas_dirty=false，session_dirty=false，因此 pending work = 0
    ///   runtime document = empty，若错误调用 replace_all_sessions([]) 会把 node_id=99 删掉
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. set_project_store(project.sqlite) 后保持无脏标记
    ///   2. 预写入 node_sessions[node_id=99]
    ///   3. flush_now(runtime empty, master_layout default)
    ///   4. load_sessions() 检查 node_id=99 是否还在
    ///
    /// 预期结果：
    ///   - 正断言：flush_now 返回 false，表示没有执行持久化工作
    ///   - 正断言：node_id=99 仍存在，证明无脏 flush 没有写 node_sessions
    ///   - 负断言：node_sessions 不应变为空，否则关闭窗口会把无关同步写放回热路径
    #[test]
    fn test_flush_now_without_dirty_state_does_not_touch_node_sessions() {
        let temp = tempfile::tempdir().expect("test should create temp dir");
        let workspace_root = temp.path().join("workspace");
        std::fs::create_dir(&workspace_root).expect("test should create workspace dir");
        let project_store =
            ProjectStore::for_workspace(&workspace_root).expect("project store should open");
        project_store
            .migrate()
            .expect("project store schema should migrate");
        let node_session_store = project_store.node_session_store();
        node_session_store
            .save_session(&NodeSession {
                node_id: 99,
                kind: NodeSessionKind::Terminal,
                cwd: Some("/tmp/stale".to_string()),
                session_id: None,
            })
            .expect("预置 node session 应写入成功");
        let mut scheduler = PersistenceScheduler::default();
        let runtime = Runtime::new();

        scheduler.set_project_store(project_store);
        let flushed = scheduler
            .flush_now(
                &runtime,
                &crate::canvas::master_layout::MasterLayoutState::default(),
            )
            .expect("无脏数据 flush 应成功返回");
        let sessions = node_session_store
            .load_sessions()
            .expect("flush 后 node_sessions 查询应成功");

        assert!(
            !flushed,
            "无 canvas/session 脏标记时 flush_now 应返回 false"
        );
        assert_eq!(
            sessions.len(),
            1,
            "无脏 flush 不应清空 node_sessions，否则关闭窗口会重新引入同步写库"
        );
        assert_eq!(
            sessions[0].node_id, 99,
            "node_id=99 应保留，证明 flush_now 没有调用 replace_all_sessions([])"
        );
        assert_eq!(
            sessions[0].cwd.as_deref(),
            Some("/tmp/stale"),
            "保留下来的 node session 应保持原 cwd，不应被空 runtime 覆盖"
        );
    }
}
