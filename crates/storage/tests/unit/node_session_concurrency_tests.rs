use super::*;
use rusqlite::ErrorCode;
use std::{sync::mpsc, time::Duration};

/// 场景：节点会话读改写与后台恢复并发，不能在读完旧会话后丢掉写锁资格。
/// 构造：同一个真实SQLite文件，历史terminal=7/cwd=historical，已删除agent=99；第二连接busy_timeout=0仅探测写锁。
/// 执行：生产reconcile读历史后，另一线程尝试BEGIN IMMEDIATE；记录结果并等reconcile返回，再验证它可取得写锁。
/// 预期：reconcile独占读改写事务、保存成功并保留历史cwd，外部writer先Busy、提交后成功，已删除99不能复活。
#[test]
fn node_session_reconcile_owns_write_intent_before_reading_history() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("sessions.sqlite");
    let connection = Connection::open(&path).unwrap();
    crate::migrations::migrate_project_db(&connection).unwrap();
    drop(connection);
    let store = NodeSessionStore::new(path.clone());
    store
        .save_session(&NodeSession {
            node_id: 7,
            kind: NodeSessionKind::Terminal,
            cwd: Some("historical".into()),
            session_id: None,
        })
        .unwrap();
    store
        .save_session(&NodeSession {
            node_id: 99,
            kind: NodeSessionKind::Agent,
            cwd: None,
            session_id: Some("removed".into()),
        })
        .unwrap();
    let (begin, requested) = mpsc::sync_channel(0);
    let (result, observed) = mpsc::sync_channel(0);
    let (release, released) = mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        let connection = Connection::open(path).unwrap();
        connection.busy_timeout(Duration::ZERO).unwrap();
        requested.recv_timeout(Duration::from_secs(5)).unwrap();
        let attempt = connection.execute_batch("BEGIN IMMEDIATE");
        let acquired = attempt.is_ok();
        let code = attempt.err().and_then(|error| error.sqlite_error_code());
        result.send(code).unwrap();
        released.recv_timeout(Duration::from_secs(5)).unwrap();
        if acquired {
            connection.execute_batch("ROLLBACK").unwrap();
        }
        connection
            .execute_batch("BEGIN IMMEDIATE; ROLLBACK")
            .expect("生产提交后，外部writer必须可取得写锁");
    });
    let mut competing = None;
    let saved = store.reconcile_sessions_after_read(&[(7, NodeSessionKind::Terminal)], &[], || {
        begin.send(()).unwrap();
        competing = Some(observed.recv_timeout(Duration::from_secs(5)).unwrap());
    });
    release.send(()).unwrap();
    worker.join().unwrap();
    assert!(
        saved.is_ok(),
        "读改写必须一次成功，不因另一个writer抢到reserved锁而返回BUSY：{saved:?}"
    );
    assert_eq!(
        competing,
        Some(Some(ErrorCode::DatabaseBusy)),
        "读取历史前就必须持有写意图，竞争writer不能抢先获得reserved锁"
    );
    let rows = store.load_sessions().unwrap();
    assert_eq!(rows.len(), 1, "仅保留当前仍存在的一个节点");
    assert_eq!(rows[0].node_id, 7, "不能把已删除的99复活");
    assert_eq!(
        rows[0].cwd.as_deref(),
        Some("historical"),
        "未启动终端的历史cwd不能丢失"
    );
    assert!(
        !rows.iter().any(|row| row.node_id == 99),
        "并发不能恢复已删除的agent会话"
    );
}
