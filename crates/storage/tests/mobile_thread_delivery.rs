use storage::{NewThreadMessage, ThreadMessageStore};

/// 持久接收回执在重启后保持不变，同操作改内容或换线程不能生成第二份接收。
/// 数据构造：A/B 两线程各 1 条消息；hash 为 64 位测试十六进制；事件页上限 32，实际仅 1 条。
/// 执行过程：接收 A → 重建 Store 重试 → 修改原文/归属/消息操作 → 接收 B → 按线程读取。
/// 预期结果：A 的事件身份和游标不变；冲突无新增行；B 不出现在 A 的事件或回执查询中。
#[test]
fn durable_receipt_survives_reopen_and_rejects_changed_identity() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let path = dir.path().join("app.sqlite");
    let a = request("operation-a", "message-a", "thread-a");
    let original = ThreadMessageStore::new(path.clone())
        .receive(&a)
        .expect("接收 A");
    let store = ThreadMessageStore::new(path);
    assert_eq!(
        store.receive(&a).expect("丢回执后重试"),
        original,
        "重启必须返回原持久回执"
    );
    let mut changed = a.clone();
    changed.payload = "{\"text\":\"篡改\"}".into();
    assert!(store.receive(&changed).is_err(), "同指纹也不能改写原载荷");
    changed = a.clone();
    changed.thread_id = "thread-other".into();
    assert!(store.receive(&changed).is_err(), "原操作不能移动线程");
    changed = a.clone();
    changed.operation_id = "operation-other".into();
    assert!(store.receive(&changed).is_err(), "同消息不能换操作再次接收");
    changed = request("operation-next", "message-next", "thread-a");
    changed.collaboration_workspace_id = "other-workspace".into();
    assert!(
        store.receive(&changed).is_err(),
        "已绑定线程不能移动 Workspace"
    );
    let b = request("operation-b", "message-b", "thread-b");
    let second = store.receive(&b).expect("接收 B");
    assert!(second.cursor > original.cursor, "新事件须有独立递增游标");
    let events = store
        .events("phone", "mac", "collaboration", "thread-a", 0)
        .expect("A 事件");
    assert_eq!(
        events.iter().map(|v| &v.receipt).collect::<Vec<_>>(),
        vec![&original],
        "A 只补回原事件且不重复"
    );
    assert!(
        !events.iter().any(|v| v.receipt.message_id == "message-b"),
        "B 的正文与回执不可串入 A"
    );
    assert!(
        store
            .get("phone", "mac", "collaboration", "thread-a", "operation-b")
            .expect("隔离查询")
            .is_none(),
        "错误线程不能读 B 回执"
    );
    assert!(
        store
            .get(
                "other-phone",
                "mac",
                "collaboration",
                "thread-a",
                "operation-a"
            )
            .expect("隔离设备")
            .is_none(),
        "其他手机不能读此设备操作"
    );
}

/// 接收事件写入失败时，消息与首次线程绑定必须一并回滚。
/// 数据构造：临时 SQLite trigger 拒绝事件 INSERT；三张表均从空开始，期望行数 0。
/// 执行过程：建库 → 安装失败 trigger → 接收 → 查三表 → 移除 trigger → 重试原操作。
/// 预期结果：失败不返回回执且无孤儿消息/线程；恢复后恰好接收一次。
#[test]
fn event_write_failure_rolls_back_the_entire_receipt() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let path = dir.path().join("app.sqlite");
    storage::AppStore::new(path.clone())
        .migrate()
        .expect("建库");
    let connection = rusqlite::Connection::open(&path).expect("检查数据库");
    connection.execute_batch("CREATE TRIGGER fail_thread_event BEFORE INSERT ON mobile_thread_events BEGIN SELECT RAISE(ABORT, 'test failure'); END;").expect("注入写失败");
    let store = ThreadMessageStore::new(path);
    let message = request("operation-a", "message-a", "thread-a");
    assert!(store.receive(&message).is_err(), "事件落盘失败不能签收");
    let count = |table: &str| {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .expect("行数")
    };
    assert_eq!(count("mobile_thread_messages"), 0, "失败不能遗留已接收消息");
    assert_eq!(count("mobile_thread_events"), 0, "失败不能遗留接收事件");
    assert_eq!(
        count("mobile_collaboration_threads"),
        0,
        "失败不能先绑定空线程"
    );
    connection
        .execute_batch("DROP TRIGGER fail_thread_event;")
        .expect("恢复磁盘写入");
    assert_eq!(
        store.receive(&message).expect("恢复后接收").state,
        "received",
        "同操作仍可正常接收"
    );
    assert_eq!(count("mobile_thread_messages"), 1, "只保存一份原文");
}

fn request(operation: &str, message: &str, thread: &str) -> NewThreadMessage {
    NewThreadMessage {
        device_id: "phone".into(),
        source_mac_id: "mac".into(),
        collaboration_workspace_id: "collaboration".into(),
        thread_id: thread.into(),
        operation_id: operation.into(),
        message_id: message.into(),
        origin_device_id: "origin".into(),
        fingerprint: "a".repeat(64),
        payload: "{\"text\":\"讨论首屏\"}".into(),
    }
}
