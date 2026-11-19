use brain::staff::{TaskEventResult, TaskStore};
use brain::{BrainEventKind, Outcome};
use norma::protocol::{Task, Worker};

/// 保存失败既不发布终态也不吞掉事件，恢复磁盘后同一事件能够重试。
///
/// 数据构造：任务 a 含 node=7/dispatch=d，一条成功结果；用同名目录模拟原子替换失败。
/// 执行过程：保存初态 → 备份快照并以目录占位 → 投递结果失败 → 恢复文件 → 重投同一结果。
/// 预期结果：失败时内存无结果且备份不变；恢复后结果只写一次并唤醒幕僚验收，重复为 Ignored。
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
        unchanged.workers[0].outcome().is_none(),
        "worker must remain unsettled for retry"
    );
    std::fs::remove_dir(&path).expect("remove fixture obstacle");
    std::fs::rename(&backup, &path).expect("restore file");
    assert_eq!(
        store.record_event("ws", &event).expect("retry"),
        TaskEventResult::Recorded {
            task_id: "a".into(),
            wake: true
        },
        "same event must be accepted after storage recovery and wake staff to review the result"
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

/// 旧版（v1）快照读不出新消息格式，升级时备份原件并从空库开始，而不是报错或丢失。
///
/// 数据构造：手写 version=1 的 tasks.json，里面是已不存在的 Norma 消息变体。
/// 执行过程：用新 Store 列出任务 → 创建一个新任务 → 再用另一个 Store 重新加载。
/// 预期结果：首次 list 为空；原件字节原样保存在 tasks.v1.bak.json；新任务写入当前版本快照后可重新加载。
#[test]
fn v1_snapshot_is_backed_up_and_store_starts_empty() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    let path = store.path("ws");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    let old = br#"{"version":1,"workspace_id":"ws","tasks":[{"id":"a","messages":[{"Norma":{"text":"hi"}}]}]}"#;
    std::fs::write(&path, old).expect("v1 fixture");
    assert!(
        store
            .list("ws")
            .expect("v1 must not be an error")
            .is_empty(),
        "升级后从空库开始"
    );
    let backup = path.with_file_name("tasks.v1.bak.json");
    assert_eq!(
        std::fs::read(&backup).expect("backup"),
        old.to_vec(),
        "原件必须原样备份"
    );
    store
        .create(Task::new("b", "ws", "project", "新任务"))
        .expect("create");
    let reloaded = TaskStore::new(dir.path().into());
    assert_eq!(
        reloaded.list("ws").expect("reload").len(),
        1,
        "新任务写入当前版本快照后可以重新加载"
    );
}

/// 实时更新只改内存、不写盘、不加 revision；落盘更新才持久化。
///
/// 数据构造：任务 a；update_live 开一条回复并追加文字「流式」。
/// 执行过程：读内存 → 用新 Store 读盘 → 再做一次普通 update。
/// 预期结果：内存里有回复且 revision 不变；磁盘上没有；普通 update 之后磁盘上才有，revision 加 1。
#[test]
fn live_updates_stay_in_memory_until_a_durable_update() {
    use norma::protocol::{ReplyTrigger, TaskMessage};
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    let created = store
        .create(Task::new("a", "ws", "project", "Ship"))
        .expect("create");
    store
        .update_live("ws", "a", |task| {
            task.begin_reply(ReplyTrigger::User).push_text("流式");
        })
        .expect("live");
    let live = store.get("ws", "a").expect("memory");
    assert_eq!(live.messages.len(), 1, "内存里能立刻看到流式回复");
    assert_eq!(live.revision, created.revision, "实时更新不加 revision");
    let disk = TaskStore::new(dir.path().into())
        .get("ws", "a")
        .expect("disk");
    assert!(disk.messages.is_empty(), "实时更新不写盘");
    let saved = store.update("ws", "a", |_| Ok(())).expect("durable");
    assert_eq!(
        saved.revision,
        created.revision + 1,
        "落盘更新才加 revision"
    );
    let disk = TaskStore::new(dir.path().into())
        .get("ws", "a")
        .expect("disk");
    assert!(
        matches!(disk.messages.as_slice(), [TaskMessage::Staff(_)]),
        "落盘后磁盘上有回复"
    );
}

/// 崩溃遗留的 Running 回复在加载时变成 Failed 并立即持久化，只修一次。
///
/// 数据构造：落盘一条 Running 回复（模拟流式中退出）。
/// 执行过程：用新 Store 加载两次（第二次换新 Store）。
/// 预期结果：第一次加载回复为 Failed 且 revision 加 1；第二次加载状态仍 Failed、revision 不再变。
#[test]
fn running_replies_fail_on_load_once() {
    use norma::protocol::{ReplyStatus, ReplyTrigger, TaskMessage};
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    store
        .create(Task::new("a", "ws", "project", "Ship"))
        .expect("create");
    let saved = store
        .update("ws", "a", |task| {
            task.begin_reply(ReplyTrigger::User).push_text("半句");
            Ok(())
        })
        .expect("save running");
    let status = |task: &Task| match &task.messages[0] {
        TaskMessage::Staff(reply) => reply.status,
        other => panic!("unexpected {other:?}"),
    };
    let first = TaskStore::new(dir.path().into())
        .get("ws", "a")
        .expect("load");
    assert_eq!(
        status(&first),
        ReplyStatus::Failed,
        "Running 回复加载后变 Failed"
    );
    assert_eq!(first.revision, saved.revision + 1, "修复算一次写入");
    let second = TaskStore::new(dir.path().into())
        .get("ws", "a")
        .expect("reload");
    assert_eq!(
        second.revision, first.revision,
        "只修一次，不反复加 revision"
    );
}
