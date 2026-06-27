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
