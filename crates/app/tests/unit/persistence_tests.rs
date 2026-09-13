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

/// P-3: 保存失败状态必须由 PersistenceScheduler 独立维护，并在下一次成功 flush 后恢复。
///
/// 数据构造（含关键数值的推导过程）：
///   初始 dirty 数 = 0，因此 save_state=Clean。
///   第一次 flush result = Err(MissingHomeDirectory)，因此 save_state=Failed。
///   第二次 flush result = Ok(true)，并清除 1 个 canvas dirty 标记，因此 save_state=Clean。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 新建默认 scheduler，确认初始状态为 Clean。
///   2. 标记 canvas dirty，并记录一次失败 flush。
///   3. 模拟成功 flush 清除 dirty，再记录 Ok(true)。
///   4. 检查失败状态已恢复，不依赖 workspace_state.error 的字符串内容。
///
/// 预期结果：
///   - 正断言：失败后为 Failed，成功后恢复 Clean。
///   - 正断言：有 dirty 且无失败时为 Pending。
///   - 负断言：成功后不得残留 Failed，否则顶部会永久显示 Save issue。
#[test]
fn test_save_state_recovers_after_successful_flush() {
    let mut scheduler = PersistenceScheduler::default();

    assert_eq!(
        scheduler.save_state(),
        PersistenceSaveState::Clean,
        "默认 scheduler 无脏数据且无错误，应显示 Clean"
    );
    scheduler.mark_canvas_dirty();
    assert_eq!(
        scheduler.save_state(),
        PersistenceSaveState::Pending,
        "标记一个 canvas dirty 后应显示 Pending"
    );

    let failed = scheduler.track_flush_result(Err(StorageError::MissingHomeDirectory));
    assert!(failed.is_err(), "失败的 flush 结果必须原样返回给调用方");
    assert_eq!(
        scheduler.save_state(),
        PersistenceSaveState::Failed,
        "记录失败 flush 后应显示 Failed"
    );

    scheduler.canvas_dirty = false;
    let recovered = scheduler.track_flush_result(Ok(true));
    assert!(
        recovered.expect("成功 flush 结果应原样返回"),
        "成功 flush 应保留 had_pending_work=true"
    );
    assert_eq!(
        scheduler.save_state(),
        PersistenceSaveState::Clean,
        "下一次成功 flush 后必须清除 Failed，恢复 Clean"
    );
    assert_ne!(
        scheduler.save_state(),
        PersistenceSaveState::Failed,
        "成功 flush 后不得永久残留 Save issue"
    );
}

/// 后台只启动部分卡片时，保存不能删除未启动终端的 cwd/grid 恢复信息。
///
/// 数据构造：1 个未启动终端，历史尺寸120×40、shell=/bin/bash；1 个已删除节点99。
///   默认80×24不等于历史120×40，因此覆盖回退默认的错误。
/// 执行过程：预置历史 checkpoint → 仅标记会话脏 → 保存 → 读回数据库。
/// 预期结果：仍存在的终端保留120×40和shell；已删除的99不能复活。
#[test]
fn background_flush_preserves_inactive_checkpoints_and_removes_deleted_nodes() {
    let temp = tempfile::tempdir().expect("隔离数据库");
    let store = ProjectStore::for_workspace(temp.path()).expect("存储");
    store.migrate().expect("schema");
    let mut runtime = Runtime::new();
    let node = runtime.document.add_node(
        CanvasNodeKind::Term(canvas::TermSpec),
        CanvasRect::new(0., 0., 640., 400.),
    );
    let checkpoint = storage::SessionCheckpoint {
        node_id: node,
        kind: storage::SessionCheckpointKind::Terminal,
        payload: "Terminal".into(),
        cols: 120,
        rows: 40,
        shell: Some("/bin/bash".into()),
        agent_profile: None,
    };
    let mut removed = checkpoint.clone();
    removed.node_id = 99;
    let mut scheduler = PersistenceScheduler::default();
    scheduler.set_project_store(store.clone());
    scheduler.retained_checkpoints = vec![checkpoint, removed];
    scheduler.mark_session_dirty();
    scheduler
        .flush_now(&runtime, &MasterLayoutState::default())
        .expect("保存");
    let saved = store.session_store().load_checkpoints().expect("读回");
    assert_eq!(saved.len(), 1, "未启动的现有节点不能被空live列表抹去");
    assert_eq!(saved[0].node_id, node, "保留准确节点");
    assert_eq!(
        (saved[0].cols, saved[0].rows),
        (120, 40),
        "保留桌面历史尺寸"
    );
    assert_eq!(saved[0].shell.as_deref(), Some("/bin/bash"), "保留原shell");
    assert!(
        !saved.iter().any(|entry| entry.node_id == 99),
        "已删除节点不能回写"
    );
    assert_eq!(runtime.live_len(), 0, "保存不能启动任何终端");
}
