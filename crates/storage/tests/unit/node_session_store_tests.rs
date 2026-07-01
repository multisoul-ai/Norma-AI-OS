//! Unit tests for NodeSessionStore.

use crate::{
    migrations::migrate_project_db,
    node_session_store::{NodeSession, NodeSessionKind, NodeSessionStore},
};
use tempfile::tempdir;

fn make_store() -> (NodeSessionStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let conn = rusqlite::Connection::open(&path).unwrap();
    migrate_project_db(&conn).unwrap();
    (NodeSessionStore::new(path), dir)
}

/// NSS-1: 同一 node_id 的两次 save 只保留一行（upsert）。
///
/// 数据构造：
///   node_id = 42
///   第一次 save: cwd = "/tmp/a"
///   第二次 save: cwd = "/tmp/b"（覆盖）
///
/// 执行过程：
///   1. 第一次 save → INSERT OR REPLACE
///   2. 第二次 save → INSERT OR REPLACE，覆盖旧行
///   3. load_sessions()
///
/// 预期结果：
///   - 返回 1 行
///   - cwd = Some("/tmp/b")
#[test]
fn test_upsert_overwrites_same_node_id() {
    let (store, _dir) = make_store();
    let session_a = NodeSession {
        node_id: 42,
        kind: NodeSessionKind::Terminal,
        cwd: Some("/tmp/a".into()),
        session_id: None,
    };
    let session_b = NodeSession {
        node_id: 42,
        kind: NodeSessionKind::Terminal,
        cwd: Some("/tmp/b".into()),
        session_id: None,
    };

    store.save_session(&session_a).unwrap();
    store.save_session(&session_b).unwrap();
    let rows = store.load_sessions().unwrap();

    assert_eq!(
        rows.len(),
        1,
        "upsert 应只保留一行，但存在 {} 行",
        rows.len()
    );
    assert_eq!(
        rows[0].cwd.as_deref(),
        Some("/tmp/b"),
        "第二次 save 应覆盖 cwd，但实际为 {:?}",
        rows[0].cwd
    );
}

/// NSS-2: cwd = None 存储为 NULL，load 后仍为 None。
///
/// 预期结果：
///   - rows[0].cwd == None
#[test]
fn test_null_cwd_round_trips() {
    let (store, _dir) = make_store();
    store
        .save_session(&NodeSession {
            node_id: 1,
            kind: NodeSessionKind::Terminal,
            cwd: None,
            session_id: None,
        })
        .unwrap();

    let rows = store.load_sessions().unwrap();
    assert!(
        rows[0].cwd.is_none(),
        "cwd=None 应保持 None，实际为 {:?}",
        rows[0].cwd
    );
}

/// NSS-3: session_id = Some("abc") 经 save/load 后完整保留。
///
/// 预期结果：
///   - rows[0].session_id == Some("abc-123")
#[test]
fn test_session_id_round_trips() {
    let (store, _dir) = make_store();
    store
        .save_session(&NodeSession {
            node_id: 5,
            kind: NodeSessionKind::Agent,
            cwd: None,
            session_id: Some("abc-123".into()),
        })
        .unwrap();

    let rows = store.load_sessions().unwrap();
    assert_eq!(
        rows[0].session_id.as_deref(),
        Some("abc-123"),
        "session_id 应完整保留"
    );
}

/// NSS-4: 空表的 load_sessions 返回空 Vec（向后兼容，迁移后首次打开）。
///
/// 预期结果：
///   - rows.len() == 0
#[test]
fn test_load_empty_returns_empty_vec() {
    let (store, _dir) = make_store();
    let rows = store.load_sessions().unwrap();
    assert!(rows.is_empty(), "空表应返回空 Vec，实际 {} 行", rows.len());
}

/// NSS-5: replace_all_sessions 应用当前 live node 集合替换整表，并删除已不存在节点的旧行。
///
/// 数据构造（含关键数值的推导过程）：
///   旧行 stale: node_id=1, kind=Agent, session_id="stale-session"
///   当前行 terminal: node_id=2, cwd="/tmp/current"
///   当前行 agent: node_id=3, session_id="current-session"
///   replace_all 后应只有 node_id=2 和 3；node_id=1 不在当前集合，必须删除
///
/// 执行过程（逐步说明系统如何处理）：
///   1. save_session(stale) 模拟历史残留 row
///   2. replace_all_sessions([terminal, agent]) 写入当前集合
///   3. load_sessions() 读取全表
///
/// 预期结果：
///   - 正断言：只读回 2 行，分别是 node_id=2/3
///   - 正断言：node_id=3 的 session_id 为 current-session
///   - 负断言：node_id=1 的 stale-session 不应继续存在
#[test]
fn test_replace_all_sessions_prunes_stale_rows() {
    let (store, _dir) = make_store();
    let stale = NodeSession {
        node_id: 1,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("stale-session".into()),
    };
    let terminal = NodeSession {
        node_id: 2,
        kind: NodeSessionKind::Terminal,
        cwd: Some("/tmp/current".into()),
        session_id: None,
    };
    let agent = NodeSession {
        node_id: 3,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("current-session".into()),
    };
    store
        .save_session(&stale)
        .expect("stale row should be inserted first");

    store
        .replace_all_sessions(&[terminal.clone(), agent.clone()])
        .expect("replace_all_sessions should replace the whole table");
    let rows = store
        .load_sessions()
        .expect("node sessions should load after replacement");

    assert_eq!(
        rows.len(),
        2,
        "replace_all_sessions 应只保留当前 2 条 row，实际为 {}",
        rows.len()
    );
    assert!(
        rows.iter().any(|row| row.node_id == 2),
        "当前 terminal node_id=2 应存在"
    );
    assert!(
        rows.iter()
            .any(|row| row.node_id == 3 && row.session_id.as_deref() == Some("current-session")),
        "当前 agent node_id=3 应保存 current-session"
    );
    assert!(
        !rows.iter().any(|row| row.node_id == 1),
        "stale node_id=1 不应继续存在，否则删除卡片后会残留旧 session row"
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.session_id.as_deref() == Some("stale-session")),
        "stale-session 不应继续存在，否则恢复/关闭路径会携带脏状态"
    );
}
