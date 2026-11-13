use brain::staff::{TaskEventResult, TaskStore};
use brain::{BrainEventKind, Outcome};
use norma::protocol::{Task, Worker};

/// 保存失败既不发布终态也不吞掉事件，恢复磁盘后同一事件能够重试。
///
/// 数据构造：任务 a 含 node=7/dispatch=d，一条成功结果；用同名目录模拟原子替换失败。
/// 执行过程：保存初态 → 备份快照并以目录占位 → 投递结果失败 → 恢复文件 → 重投同一结果。
/// 预期结果：失败时内存无结果且备份不变；恢复后结果只写一次，重复为 Ignored。
#[test]
fn failed_atomic_save_does_not_publish_or_consume_result() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    let mut task = Task::new("a", "ws", "project", "Ship");
    task.workers
        .push(Worker::new("Simon", "Codex", "Build", 7, "d"));
    store.create(task).expect("create");
    let path = store.path("ws");
    let backup = path.with_extension("backup");
    std::fs::rename(&path, &backup).expect("backup fixture");
    std::fs::create_dir(&path).expect("block replacement");
    let event = BrainEventKind::AgentFinished {
        node_id: 7,
        dispatch_id: Some("d".into()),
        agent: "Simon".into(),
        outcome: Outcome::Succeeded,
        excerpt: "tests pass".into(),
        from_result: true,
    };
    assert!(
        store.record_event("ws", &event).is_err(),
        "cannot publish before atomic save succeeds"
    );
    let unchanged = store.get("ws", "a").expect("cached original");
    assert!(
        unchanged.messages.is_empty(),
        "failed result must not appear in published snapshot"
    );
    assert!(
        unchanged.workers[0].outcome.is_none(),
        "worker must remain unsettled for retry"
    );
    std::fs::remove_dir(&path).expect("remove fixture obstacle");
    std::fs::rename(&backup, &path).expect("restore file");
    assert_eq!(
        store.record_event("ws", &event).expect("retry"),
        TaskEventResult::Recorded {
            task_id: "a".into(),
            wake: false
        },
        "same event must be accepted after storage recovery"
    );
    assert_eq!(
        store.record_event("ws", &event).expect("repeat"),
        TaskEventResult::Ignored,
        "successful retry must become idempotent"
    );
    assert_eq!(
        TaskStore::new(dir.path().into())
            .get("ws", "a")
            .expect("reload")
            .messages
            .len(),
        1,
        "exactly one result must be durable"
    );
}

/// 损坏快照必须保留原件，修复后同一 Store 可以重试加载。
///
/// 数据构造：tasks.json 含不完整 JSON；合法快照来自同一工作空间的真实 Store 创建。
/// 执行过程：保存合法文件 → 换成损坏内容 → 新 Store 尝试创建 → 恢复合法文件再读取。
/// 预期结果：损坏读取报错，不覆盖原件、不发布空列表；修复后原任务完整恢复。
#[test]
fn corrupt_snapshot_is_preserved_and_load_can_retry() {
    let dir = tempfile::tempdir().expect("root");
    let first = TaskStore::new(dir.path().into());
    first
        .create(Task::new("a", "ws", "project", "Original"))
        .expect("create original");
    let path = first.path("ws");
    let original = std::fs::read(&path).expect("saved JSON");
    std::fs::write(&path, "{broken").expect("corrupt fixture");
    let reload = TaskStore::new(dir.path().into());
    assert!(
        reload
            .create(Task::new("b", "ws", "project", "Second"))
            .is_err(),
        "corrupt snapshot must reject writes"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("original corruption"),
        "{broken",
        "corrupt bytes must be preserved"
    );
    std::fs::write(&path, original).expect("repair original fixture");
    assert_eq!(
        reload.get("ws", "a").expect("retry load").title,
        "Original",
        "retry must recover original task"
    );
    assert!(
        reload.get("ws", "b").is_err(),
        "failed create must not leak into memory"
    );
}

/// 同秒到达的异步快照仍能按 revision 排序。
/// 数据构造：同一任务，两次连续写消息；初始 revision=0，每次成功写入 +1。
/// 执行过程：创建 → 写 first → 写 second → 新 Store 读取。
/// 预期结果：second 版本高于 first，重载与 second 相同，旧快照不得等于最新版本。
#[test]
fn snapshot_revision_orders_concurrent_ui_deliveries() {
    let dir = tempfile::tempdir().expect("temp");
    let store = brain::staff::TaskStore::new(dir.path().into());
    store
        .create(norma::protocol::Task::new("task", "ws", "project", "title"))
        .expect("create");
    let first = store
        .update("ws", "task", |task| {
            task.user_message("first");
            Ok(())
        })
        .expect("first");
    let second = store
        .update("ws", "task", |task| {
            task.user_message("second");
            Ok(())
        })
        .expect("second");
    assert!(
        second.revision > first.revision,
        "latest snapshot needs a higher revision"
    );
    let read = brain::staff::TaskStore::new(dir.path().into())
        .get("ws", "task")
        .expect("reload");
    assert_eq!(read.revision, second.revision, "revision must be durable");
    assert_ne!(
        read.revision, first.revision,
        "old snapshot cannot replace the current task"
    );
}

/// 大脑换代时旧后台操作仍持有 Store，新的服务必须共享该写入口。
/// 数据构造：两个服务代引用同一根目录，旧代先读空快照，新代创建 b，旧代随后创建 a。
/// 执行过程：获取两个共享入口 → 新代写 b → 旧代写 a → 两侧读最终集合。
/// 预期结果：两个入口是同一 Arc；a/b 都保留，不能因旧代缓存覆盖而丢掉 b。
#[test]
fn service_generations_share_the_atomic_writer() {
    let dir = tempfile::tempdir().expect("root");
    let old = TaskStore::shared(dir.path().into());
    assert!(
        old.list("ws").expect("old cache").is_empty(),
        "fixture starts empty"
    );
    let next = TaskStore::shared(dir.path().into());
    next.create(Task::new("b", "ws", "p", "New generation"))
        .expect("new write");
    old.create(Task::new("a", "ws", "p", "Late old write"))
        .expect("old write");
    assert!(
        std::sync::Arc::ptr_eq(&old, &next),
        "live generations must serialize through one Store"
    );
    assert!(
        next.get("ws", "a").is_ok(),
        "late old write must be visible"
    );
    assert!(
        old.get("ws", "b").is_ok(),
        "old cache must not erase new generation data"
    );
    assert_ne!(
        next.list("ws").expect("list").len(),
        1,
        "neither generation's task may be lost"
    );
}
