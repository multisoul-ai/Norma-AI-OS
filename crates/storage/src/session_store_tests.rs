use super::*;
use canvas::AgentProfile;

/// SS-1: SessionStore 保存并读回重建 live session 所需的 checkpoint 元数据。
///
/// 数据构造（含关键数值的推导过程）：
///   terminal node_id = 7，cols=120，rows=31，shell="/bin/sh"
///   agent node_id = 8，cols=100，rows=28，profile=Codex
///   两条记录 node_id 不同，因此 load_checkpoints 应按 node_id 读回 2 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. save_checkpoint(terminal) → 插入 Terminal checkpoint
///   2. save_checkpoint(agent) → 插入 Agent checkpoint
///   3. load_checkpoints() → 读取全部 checkpoint
///
/// 预期结果：
///   - 正断言：读回 2 条 checkpoint，且 shell/profile/cols/rows 与保存值一致
///   - 负断言：Terminal 不应带 Agent profile，Agent 不应带 shell
#[test]
fn test_session_checkpoint_round_trips_restore_metadata() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = SessionStore::new(temp.path().join("project.sqlite"));
    let terminal = SessionCheckpoint {
        node_id: 7,
        kind: SessionCheckpointKind::Terminal,
        payload: "Terminal".into(),
        cols: 120,
        rows: 31,
        shell: Some("/bin/sh".into()),
        agent_profile: None,
    };
    let agent = SessionCheckpoint {
        node_id: 8,
        kind: SessionCheckpointKind::Agent,
        payload: "Codex".into(),
        cols: 100,
        rows: 28,
        shell: None,
        agent_profile: Some(AgentProfile::Codex),
    };

    store
        .save_checkpoint(&terminal)
        .expect("terminal checkpoint 应写入 SQLite");
    store
        .save_checkpoint(&agent)
        .expect("agent checkpoint 应写入 SQLite");

    let checkpoints = store
        .load_checkpoints()
        .expect("checkpoints 应可从 SQLite 读取");

    assert_eq!(
        checkpoints.len(),
        2,
        "两个不同 node_id 的 checkpoint 都应被保存并读回"
    );
    assert_eq!(
        checkpoints[0], terminal,
        "Terminal checkpoint 应完整保留 shell/cols/rows 元数据"
    );
    assert_eq!(
        checkpoints[1], agent,
        "Agent checkpoint 应完整保留 profile/cols/rows 元数据"
    );
    assert_eq!(
        checkpoints[0].agent_profile, None,
        "Terminal checkpoint 不应携带 Agent profile，避免恢复时误判类型"
    );
    assert_eq!(
        checkpoints[1].shell, None,
        "Agent checkpoint 不应携带 Terminal shell，避免恢复时误启动 shell"
    );
}

/// SS-2: SessionStore 按 node_id 覆盖 checkpoint，不追加 terminal 输出 chunk。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 7
///   first cols/rows = 80x24，payload = "first checkpoint"
///   second cols/rows = 120x30，payload = "second checkpoint"
///   两次保存同一 node_id，PRIMARY KEY(node_id) 应让记录数保持为 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. save_checkpoint(first) → 插入 node_id=7
///   2. save_checkpoint(second) → 按 node_id 覆盖 payload/尺寸/shell
///   3. load_checkpoints() → 读取全部 checkpoint
///
/// 预期结果：
///   - 正断言：只存在 1 条 checkpoint
///   - 正断言：payload、cols、rows 是 second checkpoint
///   - 负断言：first checkpoint 不应作为旧 chunk 继续存在
#[test]
fn test_session_checkpoint_overwrites_by_node_id() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = SessionStore::new(temp.path().join("project.sqlite"));
    let first = SessionCheckpoint {
        node_id: 7,
        kind: SessionCheckpointKind::Terminal,
        payload: "first checkpoint".into(),
        cols: 80,
        rows: 24,
        shell: Some("/bin/sh".into()),
        agent_profile: None,
    };
    let second = SessionCheckpoint {
        node_id: 7,
        kind: SessionCheckpointKind::Terminal,
        payload: "second checkpoint".into(),
        cols: 120,
        rows: 30,
        shell: Some("/bin/zsh".into()),
        agent_profile: None,
    };

    store
        .save_checkpoint(&first)
        .expect("first checkpoint 应写入 SQLite");
    store
        .save_checkpoint(&second)
        .expect("second checkpoint 应覆盖同 node_id 记录");

    let checkpoints = store
        .load_checkpoints()
        .expect("checkpoints 应可从 SQLite 读取");

    assert_eq!(
        checkpoints.len(),
        1,
        "同一 node_id 的 checkpoint 应覆盖而不是追加"
    );
    assert_eq!(
        checkpoints[0], second,
        "读回 checkpoint 应是第二次保存的最新内容"
    );
    assert!(
        !checkpoints.iter().any(|checkpoint| checkpoint == &first),
        "旧 first checkpoint 不应继续存在，否则说明在按 chunk 追加"
    );
}

/// SS-3: 旧 session_checkpoints 表缺少恢复字段时，migration 应补齐默认列。
///
/// 数据构造（含关键数值的推导过程）：
///   legacy table columns = node_id/kind/payload/updated_at_unix
///   legacy node_id = 3，kind = terminal
///   默认终端尺寸 cols=80、rows=24，用于旧库缺失尺寸字段时安全重建
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 手动创建旧 schema 并插入 node_id=3
///   2. SessionStore::load_checkpoints() → open_project_connection 执行 migration
///   3. 读取 checkpoint，检查新增列默认值
///
/// 预期结果：
///   - 正断言：旧记录仍能读回，默认尺寸为 80x24
///   - 负断言：旧记录不应凭空获得 shell 或 Agent profile
#[test]
fn test_session_checkpoint_migrates_legacy_table_with_defaults() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let db_path = temp.path().join("project.sqlite");
    {
        let connection = rusqlite::Connection::open(&db_path).expect("legacy db should open");
        connection
            .execute_batch(
                r#"
                CREATE TABLE session_checkpoints (
                    node_id INTEGER PRIMARY KEY NOT NULL,
                    kind TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    updated_at_unix INTEGER NOT NULL
                );
                INSERT INTO session_checkpoints (node_id, kind, payload, updated_at_unix)
                VALUES (3, 'terminal', 'Terminal', 1);
                "#,
            )
            .expect("legacy session_checkpoints table should be created");
    }
    let store = SessionStore::new(db_path);

    let checkpoints = store
        .load_checkpoints()
        .expect("legacy checkpoint 应在 migration 后可读取");

    assert_eq!(
        checkpoints.len(),
        1,
        "legacy 表中已有的 checkpoint 不应在 migration 后丢失"
    );
    assert_eq!(
        checkpoints[0].node_id, 3,
        "legacy checkpoint 的 node_id=3 应保持不变"
    );
    assert_eq!(
        checkpoints[0].cols, 80,
        "legacy checkpoint 缺少 cols 时应使用默认 80 列"
    );
    assert_eq!(
        checkpoints[0].rows, 24,
        "legacy checkpoint 缺少 rows 时应使用默认 24 行"
    );
    assert_eq!(
        checkpoints[0].shell, None,
        "legacy checkpoint 不应凭空获得 shell，runtime 会按默认 shell 兜底"
    );
    assert_eq!(
        checkpoints[0].agent_profile, None,
        "legacy terminal checkpoint 不应凭空获得 Agent profile"
    );
}
