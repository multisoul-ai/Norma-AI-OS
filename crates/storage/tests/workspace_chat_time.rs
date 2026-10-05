use storage::WorkspaceChatStore;

/// 场景：重连恢复群聊时，各条事件保留 Mac 首次记录的时间。
/// 数据构造：一个操作含接收和完成两条事件；重试使用相同操作及原文，不新增事件。
/// 执行过程：接收 → 完成 → 重建 Store 并重复接收 → 比较原时间和条数。
/// 预期结果：时间大于 0 且重读一致；重复发送不能更改时间或增加事件。
#[test]
fn reconnect_retains_original_event_time() {
    let directory = tempfile::tempdir().expect("应创建隔离目录");
    let path = directory.path().join("app.sqlite");
    let store = WorkspaceChatStore::new(path.clone());
    store
        .receive("phone", "mac", "space", "op", "{\"text\":\"讨论\"}")
        .expect("应接收讨论");
    store.claim("phone", "op").expect("应领取消息");
    store
        .finish("phone", "op", "succeeded", "{\"text\":\"答复\"}")
        .expect("应保存答复");
    let first = store.events("mac", "space", 0).expect("应读取消息");
    assert!(
        first[0].created_at_unix_ms > 0,
        "接收时间必须来自 Mac，不能在重连时虚构"
    );
    assert!(
        first[1].created_at_unix_ms >= first[0].created_at_unix_ms,
        "回复不能早于接收"
    );
    let restored = WorkspaceChatStore::new(path);
    restored
        .receive("phone", "mac", "space", "op", "{\"text\":\"讨论\"}")
        .expect("重复接收应幂等");
    let second = restored.events("mac", "space", 0).expect("应恢复历史");
    assert_eq!(second.len(), 2, "重连不能追加虚假重复消息");
    assert_eq!(
        second[0].created_at_unix_ms, first[0].created_at_unix_ms,
        "原接收时间必须保留"
    );
    assert_eq!(
        second[1].created_at_unix_ms, first[1].created_at_unix_ms,
        "原答复时间必须保留"
    );
}

/// 场景：多个创建调用首次打开旧群聊数据库时，时间字段只能迁移一次。
/// 数据构造：旧版群聊表缺少时间字段；1条旧事件；4个后台调用在同一屏障后接收同一创建操作。
/// 执行过程：构造旧库 → 并发打开并迁移 → 读取4份回执 → 检查字段及旧事件。
/// 预期结果：所有调用成功且仅1列时间字段；旧事件保留时间0；未产生第二次创建操作或其它设备回执。
#[test]
fn concurrent_creation_migrates_legacy_chat_time_once() {
    let directory = tempfile::tempdir().expect("隔离并发迁移库");
    let path = directory.path().join("app.sqlite");
    let legacy = rusqlite::Connection::open(&path).expect("旧连接");
    legacy.execute_batch("CREATE TABLE workspace_chat_operations(device_id TEXT NOT NULL,operation_id TEXT NOT NULL,mac_id TEXT NOT NULL,workspace_id TEXT NOT NULL,payload TEXT NOT NULL,state TEXT NOT NULL,result TEXT,PRIMARY KEY(device_id,operation_id));
        CREATE TABLE workspace_chat_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,device_id TEXT NOT NULL,operation_id TEXT NOT NULL,mac_id TEXT NOT NULL,workspace_id TEXT NOT NULL,kind TEXT NOT NULL,payload TEXT NOT NULL);
        INSERT INTO workspace_chat_operations VALUES('legacy-phone','legacy-op','mac','space','{}','succeeded',NULL);
        INSERT INTO workspace_chat_events(device_id,operation_id,mac_id,workspace_id,kind,payload) VALUES('legacy-phone','legacy-op','mac','space','received','{}');").expect("旧表和原事件");
    drop(legacy);
    // 4个参与者全部到达屏障后并行打开，不预先初始化新版表。
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let callers: Vec<_> = (0..4)
        .map(|_| {
            let barrier = barrier.clone();
            let store = storage::AgentCreationStore::new(path.clone());
            std::thread::spawn(move || {
                barrier.wait();
                store.receive("phone", "mac", "space", "create", "{}")
            })
        })
        .collect();
    let outcomes: Vec<_> = callers
        .into_iter()
        .map(|c| c.join().expect("后台创建线程"))
        .collect();
    assert!(
        outcomes.iter().all(Result::is_ok),
        "并发首次迁移不能因重复时间字段导致创建失败：{outcomes:?}"
    );
    let conn = rusqlite::Connection::open(&path).expect("读取迁移后的库");
    let columns: i64 = conn.query_row("SELECT count(*) FROM pragma_table_info('workspace_chat_events') WHERE name='created_at_unix_ms'", [], |r| r.get(0)).expect("列数");
    assert_eq!(columns, 1, "时间字段应仅有一份");
    let event = storage::WorkspaceChatStore::new(path.clone())
        .events("mac", "space", 0)
        .expect("旧事件");
    assert_eq!(event.len(), 1, "旧记录不得丢失或重复");
    assert_eq!(event[0].operation_id, "legacy-op", "迁移保留真实旧来源");
    assert_eq!(
        event[0].created_at_unix_ms, 0,
        "旧时间未知时不得伪造当前时间"
    );
    let operations: i64 = conn
        .query_row("SELECT count(*) FROM agent_creation_operations", [], |r| {
            r.get(0)
        })
        .expect("创建操作数");
    assert_eq!(operations, 1, "同操作的4份接收只能持久保存一次");
    assert!(
        storage::AgentCreationStore::new(path)
            .receipt("other-phone", "mac", "space", "create")
            .expect("另一个设备")
            .is_none(),
        "不能串设备读取创建回执"
    );
}
