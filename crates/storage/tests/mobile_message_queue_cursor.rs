use storage::{MobileCommandReservation, MobileCommandStore, NewMobileCommand, NewQueuedMessage};

fn request(operation: &str) -> NewMobileCommand {
    NewMobileCommand {
        device_id: "phone-a".into(),
        operation_id: operation.into(),
        workspace_id: "project-a".into(),
        route_id: "route-a".into(),
        owner_id: "owner-a".into(),
        request_hash: "a".repeat(64),
    }
}

fn cursor_message(text: &str) -> NewQueuedMessage {
    NewQueuedMessage {
        node_id: 7,
        profile: "Cursor".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        grant_revision: 2,
        text: text.into(),
    }
}

/// 旧版队列表升级时必须保留未发送正文，并允许 Cursor 使用同一持久化 FIFO。
///
/// 数据构造：先创建完整AppDB，再把空队列表替换成仅允许Codex/Claude的旧schema；旧Claude消息1条，新Cursor消息1条。
/// 执行过程：重开store触发迁移 → 读取旧消息 → 入队Cursor消息 → 再次按顺序读取。
/// 预期结果：旧正文和序号不丢失；Cursor通过新约束入队；两条消息都保持queued且Claude在前。
#[test]
fn queue_migrates_legacy_profile_constraint_and_accepts_cursor() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let path = dir.path().join("app.sqlite");
    let store = MobileCommandStore::new(path.clone());
    let legacy = request("legacy-claude");
    assert!(
        matches!(
            store.reserve(&legacy).expect("创建完整AppDB"),
            MobileCommandReservation::New
        ),
        "旧消息对应的命令身份必须先登记"
    );
    let connection = rusqlite::Connection::open(&path).expect("打开旧数据库");
    connection
        .execute_batch(
            "DROP TABLE mobile_message_queue;
             CREATE TABLE mobile_message_queue (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id TEXT NOT NULL,
                operation_id TEXT NOT NULL,
                node_id INTEGER NOT NULL CHECK(node_id > 0),
                profile TEXT NOT NULL CHECK(profile IN ('Codex','Claude')),
                run_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                grant_revision INTEGER NOT NULL CHECK(grant_revision > 0),
                message TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('queued','dispatching','sent','cancelled','outcome_unknown')),
                result_json TEXT,
                error TEXT,
                UNIQUE(device_id,operation_id),
                FOREIGN KEY(device_id,operation_id) REFERENCES mobile_commands(device_id,operation_id)
             );
             INSERT INTO mobile_message_queue
                (sequence,device_id,operation_id,node_id,profile,run_id,session_id,grant_revision,message,state)
             VALUES (41,'phone-a','legacy-claude',7,'Claude','run-a','session-a',2,'legacy body','queued');",
        )
        .expect("构造旧版队列表");
    drop(connection);

    let reopened = MobileCommandStore::new(path.clone());
    let before = reopened
        .queued_messages("owner-a")
        .expect("升级后读取旧消息");
    assert_eq!(before.len(), 1, "迁移不能删除未发送的旧消息");
    assert_eq!(
        before[0].message.text, "legacy body",
        "迁移必须保留精确正文"
    );
    assert_eq!(
        before[0].message.profile, "Claude",
        "迁移不能改写旧Provider"
    );

    let cursor = request("cursor-next");
    assert!(
        matches!(
            reopened
                .enqueue_message(&cursor, &cursor_message("cursor body"))
                .expect("Cursor入队"),
            MobileCommandReservation::New
        ),
        "升级后的数据库必须接受Cursor消息"
    );
    let after = reopened.queued_messages("owner-a").expect("读取两条消息");
    assert_eq!(after.len(), 2, "旧Claude和新Cursor消息都必须保留");
    assert_eq!(after[0].message.profile, "Claude", "旧序号41必须仍排在前面");
    assert_eq!(
        after[1].message.profile, "Cursor",
        "新消息必须保留Cursor身份"
    );
    assert_eq!(after[1].state, "queued", "入队不能伪造Provider已接收");
    let connection = rusqlite::Connection::open(path).expect("核对迁移后序号");
    let sequences = connection
        .prepare("SELECT sequence FROM mobile_message_queue ORDER BY sequence")
        .expect("准备序号查询")
        .query_map([], |row| row.get::<_, i64>(0))
        .expect("查询迁移序号")
        .collect::<Result<Vec<_>, _>>()
        .expect("读取迁移序号");
    assert_eq!(sequences, vec![41, 42], "迁移必须保留旧序号并延续自增值");
}
