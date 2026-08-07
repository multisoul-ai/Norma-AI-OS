use super::*;
use canvas::{
    CanvasNodeKind, CanvasNodeSnapshot, CanvasRect, DocumentSnapshot, RectNode, Vec2, Viewport,
};
use storage::{CanvasStore, SessionCheckpoint, SessionCheckpointKind, SessionStore};

/// RS-1: Runtime 只在显式调用 save_canvas_snapshot 时写入 CanvasStore。
///
/// 数据构造（含关键数值的推导过程）：
///   runtime 初始 document 为空，添加 Rect 节点后 node_id=1
///   project db = temp/project.sqlite，初始没有 canvas_snapshots 行
///   bounds = (10,20,100,80)，viewport 保持默认 pan(0,0)/zoom=1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Runtime::new() 并 add_node(Rect)
///   2. 调用 drain_all()、send_node_input(999, Text) 等热路径附近方法，不传 Store
///   3. load_snapshot() → 仍为 None，证明未隐式写库
///   4. save_canvas_snapshot(store) → 显式写入快照
///   5. load_snapshot() → 读回包含 node_id=1 的快照
///
/// 预期结果：
///   - 正断言：显式保存前没有 canvas snapshot，显式保存后存在
///   - 正断言：快照中保留 node_id=1
///   - 负断言：drain_all/send_node_input 不应隐式创建 snapshot
#[test]
fn test_runtime_canvas_snapshot_is_saved_only_by_explicit_call() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = CanvasStore::new(temp.path().join("project.sqlite"));
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
        CanvasRect::new(10.0, 20.0, 100.0, 80.0),
    );

    let changed = runtime.drain_all();
    let missing_input = runtime.send_node_input(999, NodeInput::Text("noop".into()));
    let before = store
        .load_snapshot()
        .expect("空 project db 应可查询 canvas snapshot");

    runtime
        .save_canvas_snapshot(&store)
        .expect("显式 save_canvas_snapshot 应写入 SQLite");
    let after = store
        .load_snapshot()
        .expect("保存后应可查询 canvas snapshot")
        .expect("显式保存后 canvas snapshot 应存在");

    assert!(!changed, "空 runtime drain_all 不应产生 dirty 状态");
    assert!(
        matches!(missing_input, Err(NodeError::MissingNode)),
        "send_node_input 缺失节点应返回 MissingNode，实际: {:?}",
        missing_input
    );
    assert!(
        before.is_none(),
        "未显式保存前不应存在 canvas snapshot，热路径不应偷偷写库"
    );
    assert_eq!(
        after.nodes[0].id, node_id,
        "显式保存后的 snapshot 应包含刚创建的 node_id"
    );
    assert_ne!(
        before,
        Some(after),
        "显式保存前后状态不应相同，否则无法证明写入由 save_canvas_snapshot 触发"
    );
}

/// RS-2: Runtime 从 CanvasStore 恢复快照后保留节点并清空 live/focus 状态。
///
/// 数据构造（含关键数值的推导过程）：
///   snapshot.nodes = [id=8,z=3,bounds(1,2,300,200)]
///   viewport.pan = (9,10)，zoom = 1.75
///   恢复后 next_id 应为 max_id(8)+1 = 9
///
/// 执行过程（逐步说明系统如何处理）：
///   1. CanvasStore::save_snapshot(snapshot)
///   2. Runtime::load_canvas_snapshot(store) → 调用 restore_canvas_snapshot
///   3. add_node(Rect) → 新节点 id 应为 9
///
/// 预期结果：
///   - 正断言：load_canvas_snapshot 返回 true，节点 id=8 与 viewport 被恢复
///   - 正断言：新增节点 id=9
///   - 负断言：live_len 不应非零，恢复快照不应伪造 live process
#[test]
fn test_runtime_load_canvas_snapshot_restores_document_without_live_nodes() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = CanvasStore::new(temp.path().join("project.sqlite"));
    let snapshot = DocumentSnapshot {
        nodes: vec![CanvasNodeSnapshot {
            id: 8,
            kind: CanvasNodeKind::Rect(RectNode { color: 0x35d07f }),
            bounds: CanvasRect::new(1.0, 2.0, 300.0, 200.0),
            z_index: 3,
        }],
        viewport: Viewport {
            pan: Vec2::new(9.0, 10.0),
            zoom: 1.75,
        },
    };
    store
        .save_snapshot(&snapshot)
        .expect("测试快照应能写入 CanvasStore");
    let mut runtime = Runtime::new();

    let loaded = runtime
        .load_canvas_snapshot(&store)
        .expect("Runtime 应能从 CanvasStore 显式恢复快照");
    let next_id = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0xff8040 }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );

    assert!(loaded, "存在 snapshot 时 load_canvas_snapshot 应返回 true");
    assert!(
        runtime.document.node(8).is_some(),
        "恢复后 document 应包含 snapshot 中的 id=8 节点"
    );
    assert_eq!(
        runtime.document.viewport.zoom, 1.75,
        "恢复后 viewport.zoom 应来自 snapshot"
    );
    assert_eq!(next_id, 9, "恢复 max_id=8 后新增节点 id 应为 9");
    assert_eq!(
        runtime.live_len(),
        0,
        "恢复 canvas snapshot 不应伪造 live node/process"
    );
}

/// RS-3: Runtime session checkpoint 通过显式方法按当前 live 节点合并保存。
///
/// 数据构造（含关键数值的推导过程）：
///   spawn_terminal 初始 node_id=1，session checkpoint node_id 应为 1
///   SessionStore 使用 PRIMARY KEY(node_id)，重复 save_session_checkpoints 不应追加记录
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Runtime::spawn_terminal("/bin/sh") → 注册 1 个 live terminal
///   2. save_session_checkpoints(store) → 保存 node_id=1 checkpoint
///   3. 再次 save_session_checkpoints(store) → 覆盖同 node_id checkpoint
///   4. load_checkpoints() → 只应读到 1 条
///
/// 预期结果：
///   - 正断言：checkpoint 数量为 1，kind 为 Terminal
///   - 正断言：checkpoint.node_id 等于 spawn 返回的 node_id
///   - 负断言：重复保存不应产生第二条 checkpoint
#[test]
fn test_runtime_save_session_checkpoints_coalesces_by_node_id() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = SessionStore::new(temp.path().join("project.sqlite"));
    let mut runtime = Runtime::new();
    let node_id = runtime
        .spawn_terminal(CanvasRect::new(0.0, 0.0, 400.0, 300.0), "/bin/sh", 80, 24)
        .expect("spawn_terminal 应成功用于 session checkpoint 测试");

    runtime
        .save_session_checkpoints(&store)
        .expect("第一次 session checkpoint 保存应成功");
    runtime
        .save_session_checkpoints(&store)
        .expect("第二次 session checkpoint 保存应覆盖同 node_id");
    let checkpoints = store
        .load_checkpoints()
        .expect("session checkpoints 应可读取");

    runtime.kill_all();
    assert_eq!(
        checkpoints.len(),
        1,
        "重复保存同一个 live node checkpoint 后仍应只有 1 条记录"
    );
    assert_eq!(
        checkpoints[0].node_id, node_id,
        "checkpoint node_id 应等于 spawn_terminal 返回的 node_id"
    );
    assert_eq!(
        checkpoints[0].kind,
        SessionCheckpointKind::Terminal,
        "terminal live node 应保存为 Terminal checkpoint kind"
    );
    assert_ne!(
        checkpoints.len(),
        2,
        "session checkpoint 不应按输出 chunk 或重复保存追加第二条"
    );
}

/// RS-4: Runtime 恢复 canvas snapshot 后，可按 checkpoint 为原 node_id 重建 live terminal。
///
/// 数据构造（含关键数值的推导过程）：
///   snapshot.nodes = [Term id=8,z=3,bounds(1,2,300,200)]
///   checkpoint.node_id = 8，与 snapshot 中的 Term 节点匹配
///   checkpoint cols=90、rows=25，shell="/bin/sh"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. CanvasStore::save_snapshot(snapshot)
///   2. Runtime::load_canvas_snapshot(store) → 恢复 document，但 live_len=0
///   3. restore_live_sessions([checkpoint]) → 为 node_id=8 spawn live terminal
///   4. node_view(8) → 应返回 Term 快照，而不是 None
///
/// 预期结果：
///   - 正断言：恢复数量为 1，live_len=1，node_view(8) 为 Some
///   - 正断言：终端尺寸来自 checkpoint 的 90x25
///   - 负断言：不应新建额外 document 节点或把原 node_id 变成 session gone
#[test]
fn test_runtime_restore_live_sessions_rebinds_terminal_to_existing_node_id() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = CanvasStore::new(temp.path().join("project.sqlite"));
    let snapshot = DocumentSnapshot {
        nodes: vec![CanvasNodeSnapshot {
            id: 8,
            kind: CanvasNodeKind::Term(canvas::TermSpec),
            bounds: CanvasRect::new(1.0, 2.0, 300.0, 200.0),
            z_index: 3,
        }],
        viewport: Viewport {
            pan: Vec2::new(9.0, 10.0),
            zoom: 1.75,
        },
    };
    let checkpoint = SessionCheckpoint {
        node_id: 8,
        kind: SessionCheckpointKind::Terminal,
        payload: "Terminal".into(),
        cols: 90,
        rows: 25,
        shell: Some("/bin/sh".into()),
        agent_profile: None,
    };
    store
        .save_snapshot(&snapshot)
        .expect("测试快照应能写入 CanvasStore");
    let mut runtime = Runtime::new();

    let loaded = runtime
        .load_canvas_snapshot(&store)
        .expect("Runtime 应能从 CanvasStore 显式恢复快照");
    let restored = runtime.restore_live_sessions(&[checkpoint]);
    let view = runtime.node_view(8);

    assert!(loaded, "存在 snapshot 时 load_canvas_snapshot 应返回 true");
    assert_eq!(
        restored, 1,
        "checkpoint 匹配 snapshot Term 节点时应恢复 1 个 live terminal"
    );
    assert_eq!(
        runtime.live_len(),
        1,
        "恢复 live session 后 live registry 应包含原 node_id=8 的 terminal"
    );
    let Some(NodeView::Term { snapshot, .. }) = view else {
        runtime.kill_all();
        panic!("node_view(8) 应为 Some(NodeView::Term)，否则 UI 会显示 session gone");
    };
    assert_eq!(
        snapshot.cols, 90,
        "恢复后的 terminal cols 应来自 checkpoint 的 90 列"
    );
    assert_eq!(
        snapshot.lines, 25,
        "恢复后的 terminal rows 应来自 checkpoint 的 25 行"
    );
    assert_eq!(
        runtime.document.nodes().len(),
        1,
        "restore_live_sessions 不应额外创建 document 节点"
    );
    assert!(
        runtime.document.node(8).is_some(),
        "restore_live_sessions 应保留原 node_id=8，而不是新建替代节点"
    );
    assert!(
        runtime.node_view(99).is_none(),
        "未存在的 node_id=99 不应被伪造成 live node"
    );
    runtime.kill_all();
}

/// RS-5: checkpoint 指向不存在的节点时，Runtime 不应创建孤儿 live node。
///
/// 数据构造（含关键数值的推导过程）：
///   snapshot.nodes = [Rect id=8]，没有 Terminal/Agent 节点
///   checkpoint.node_id = 99，document 中不存在
///   初始 live_len = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. restore_canvas_snapshot(snapshot) → document 只有 Rect id=8
///   2. restore_live_sessions([node_id=99]) → checkpoint 找不到目标节点，应跳过
///   3. 查询 live_len、node_view(99)、document.node(99)
///
/// 预期结果：
///   - 正断言：恢复数量为 0，Rect id=8 仍存在
///   - 负断言：live_len 不应增加，node_id=99 不应出现在 document 或 live map
#[test]
fn test_runtime_restore_live_sessions_skips_missing_checkpoint_node() {
    let snapshot = DocumentSnapshot {
        nodes: vec![CanvasNodeSnapshot {
            id: 8,
            kind: CanvasNodeKind::Rect(RectNode { color: 0x35d07f }),
            bounds: CanvasRect::new(1.0, 2.0, 300.0, 200.0),
            z_index: 3,
        }],
        viewport: Viewport {
            pan: Vec2::new(0.0, 0.0),
            zoom: 1.0,
        },
    };
    let checkpoint = SessionCheckpoint {
        node_id: 99,
        kind: SessionCheckpointKind::Terminal,
        payload: "Terminal".into(),
        cols: 80,
        rows: 24,
        shell: Some("/bin/sh".into()),
        agent_profile: None,
    };
    let mut runtime = Runtime::new();

    runtime.restore_canvas_snapshot(snapshot);
    let restored = runtime.restore_live_sessions(&[checkpoint]);

    assert_eq!(
        restored, 0,
        "checkpoint 指向不存在的 node_id=99 时不应恢复任何 live session"
    );
    assert_eq!(
        runtime.live_len(),
        0,
        "跳过缺失节点 checkpoint 后 live registry 应保持为空"
    );
    assert!(
        runtime.document.node(8).is_some(),
        "已有 Rect id=8 不应被 restore_live_sessions 误删"
    );
    assert!(
        runtime.document.node(99).is_none(),
        "restore_live_sessions 不应为缺失 checkpoint 新建 document node"
    );
    assert!(
        runtime.node_view(99).is_none(),
        "缺失 checkpoint 不应产生 node_view(99)，否则会形成孤儿 live node"
    );
}

/// RS-6: 缺少 checkpoint 的旧库布局，也应按默认终端参数恢复可交互 live node。
///
/// 数据构造（含关键数值的推导过程）：
///   snapshot.nodes = [Term id=4]
///   checkpoints = []，模拟旧库只有 canvas_snapshots、没有 session_checkpoints
///   默认终端尺寸 = 80x24，用于避免旧库显示 session gone
///
/// 执行过程（逐步说明系统如何处理）：
///   1. restore_canvas_snapshot(snapshot) → document 有 Term id=4，live_len=0
///   2. restore_live_sessions([]) → 对缺失 checkpoint 的 Term 节点走默认恢复
///   3. node_view(4) → 应返回 Term 快照
///
/// 预期结果：
///   - 正断言：恢复数量为 1，node_view(4) 为 Some
///   - 正断言：默认尺寸为 80x24
///   - 负断言：缺少 checkpoint 不应导致 node_view(4) 为 None
#[test]
fn test_runtime_restore_live_sessions_uses_default_for_legacy_terminal_without_checkpoint() {
    let snapshot = DocumentSnapshot {
        nodes: vec![CanvasNodeSnapshot {
            id: 4,
            kind: CanvasNodeKind::Term(canvas::TermSpec),
            bounds: CanvasRect::new(1.0, 2.0, 300.0, 200.0),
            z_index: 3,
        }],
        viewport: Viewport {
            pan: Vec2::new(0.0, 0.0),
            zoom: 1.0,
        },
    };
    let mut runtime = Runtime::new();

    runtime.restore_canvas_snapshot(snapshot);
    let restored = runtime.restore_live_sessions(&[]);
    let view = runtime.node_view(4);

    assert_eq!(
        restored, 1,
        "旧库缺少 checkpoint 时，Term id=4 仍应按默认参数恢复 live session"
    );
    let Some(NodeView::Term { snapshot, .. }) = view else {
        runtime.kill_all();
        panic!("缺少 checkpoint 的旧 Term 节点也应恢复 node_view，避免 session gone");
    };
    assert_eq!(snapshot.cols, 80, "旧库默认恢复 terminal 时 cols 应为 80");
    assert_eq!(snapshot.lines, 24, "旧库默认恢复 terminal 时 rows 应为 24");
    assert!(
        runtime.node_view(4).is_some(),
        "旧库默认恢复后 node_view(4) 不应为 None"
    );
    assert!(
        runtime.node_view(5).is_none(),
        "恢复旧库 Term id=4 不应额外创建 id=5 的 live node"
    );
    runtime.kill_all();
}

/// RS-8: restore_live_sessions_with_node_sessions 当 NodeSession.session_id = None 时，
/// 不追加 --resume，使用原始 agent 命令。
///
/// 数据构造（含关键数值推导）：
///   NodeSession { kind: Agent, session_id: None }
///
/// 执行过程：
///   1. 调用 build_agent_restore_command_for_test(session, Claude, None)
///   2. 检查返回的 PtyCommand.args 不含 "--resume"
///
/// 预期结果：
///   - 负断言：session_id=None 时 args 不应包含 "--resume"
#[test]
fn test_restore_agent_no_session_id_omits_resume() {
    use storage::{NodeSession, NodeSessionKind};

    let session = NodeSession {
        node_id: 1,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: None,
    };
    let cmd = crate::session_restore::build_agent_restore_command_for_test(
        &session,
        canvas::AgentProfile::Claude,
        None,
        None,
    );
    let args_str = cmd.args.join(" ");
    assert!(
        !args_str.contains("--resume"),
        "session_id=None 时不应出现 --resume，实际 args: {args_str}"
    );
}

/// RS-9: restore_live_sessions_with_node_sessions 当 cwd = None 时不报错，
/// terminal 使用默认 shell 命令（无 cwd 覆盖）。
///
/// 数据构造（含关键数值推导）：
///   NodeSession { kind: Terminal, cwd: None, session_id: None }
///
/// 执行过程：
///   1. 调用 build_terminal_restore_command_for_test(session, "/bin/zsh")
///   2. 检查返回的 PtyCommand.cwd 为 None
///
/// 预期结果：
///   - 正断言：cwd=None 时 PtyCommand.cwd 也应为 None（回退到 workspace_root）
#[test]
fn test_restore_terminal_null_cwd_does_not_error() {
    use storage::{NodeSession, NodeSessionKind};

    let session = NodeSession {
        node_id: 1,
        kind: NodeSessionKind::Terminal,
        cwd: None,
        session_id: None,
    };
    let cmd = crate::session_restore::build_terminal_restore_command_for_test(&session, "/bin/zsh");
    assert!(
        cmd.cwd.is_none(),
        "cwd=None 时 PtyCommand.cwd 也应为 None，实际: {:?}",
        cmd.cwd
    );
}
