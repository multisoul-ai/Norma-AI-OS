//! v3 快照丢掉旧 Worker 字段，事件按卡片和派发编号成对定位。
use brain::staff::{TaskEventResult, TaskStore};
use brain::{BrainEventKind, Outcome};
use norma::protocol::{Task, Worker, WorkerOutcome, WorkerPhase};

/// 场景：磁盘上还是 v2 任务文件，里面的 Worker 仍是 state/outcome 字段。
/// 数据构造：手写 version=2 的 tasks.json，含一条已完成任务。
/// 执行过程：用新 Store 列出任务。
/// 预期结果：列表为空；原件字节原样在 tasks.v2.bak.json；tasks.json 不再是那份旧文件。
#[test]
fn v2_snapshot_is_backed_up_and_store_starts_empty() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    let path = store.path("ws");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    let old = br#"{"version":2,"workspace_id":"ws","tasks":[{"id":"a","title":"Old","workers":[{"state":"Active","outcome":null}]}]}"#;
    std::fs::write(&path, old).expect("v2 fixture");
    assert!(
        store
            .list("ws")
            .expect("v2 must not be an error")
            .is_empty(),
        "v2 不迁移，从空库开始"
    );
    assert_eq!(
        std::fs::read(path.with_file_name("tasks.v2.bak.json")).expect("v2 backup"),
        old.to_vec(),
        "v2 原件必须原样备份"
    );
    assert!(!path.exists(), "旧文件被改名备份，不会留在 tasks.json");
    store
        .create(Task::new("b", "ws", "project", "新任务"))
        .expect("create");
    let fresh = std::fs::read(&path).expect("v3 snapshot");
    assert!(
        fresh
            .windows(br#""version":3"#.len())
            .any(|window| window == br#""version":3"#),
        "新任务写入 v3"
    );
}

/// 场景：两张任务先后用过同一张卡，派发编号不同。
/// 数据构造：任务 A 的 Worker 是 card 7 / d-a，任务 B 是 card 7 / d-b，都还在 Reserved。
/// 执行过程：投递 d-a 的成功结果；再投递没有派发编号的同卡事件；再投递 d-b 的失败。
/// 预期结果：d-a 只结束 A；缺编号被忽略且两边都不变；d-b 只结束 B。
#[test]
fn events_follow_card_and_dispatch_together() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    let mut a = Task::new("a", "ws", "project", "A");
    a.workers
        .push(Worker::new("Alan", "Codex", "Build", 7, "d-a"));
    let mut b = Task::new("b", "ws", "project", "B");
    b.workers
        .push(Worker::new("Bella", "Claude", "Build", 7, "d-b"));
    store.create(a).expect("create a");
    store.create(b).expect("create b");
    let finished = |dispatch: Option<&str>, outcome| BrainEventKind::AgentFinished {
        node_id: 7,
        dispatch_id: dispatch.map(str::to_owned),
        agent: "Alan".into(),
        outcome,
        excerpt: "done".into(),
        from_result: true,
    };
    assert_eq!(
        store
            .record_event("ws", &finished(Some("d-a"), Outcome::Succeeded))
            .expect("finish a"),
        TaskEventResult::Recorded {
            task_id: "a".into(),
            wake: true,
        },
        "d-a 必须落到任务 A"
    );
    assert_eq!(
        store
            .record_event("ws", &finished(None, Outcome::Failed))
            .expect("missing dispatch"),
        TaskEventResult::Ignored,
        "缺派发编号不能猜任务"
    );
    let a = store.get("ws", "a").expect("task a");
    let b = store.get("ws", "b").expect("task b");
    assert_eq!(
        a.workers[0].phase,
        WorkerPhase::Finished(WorkerOutcome::Succeeded),
        "A 收到自己的结果"
    );
    assert_eq!(
        b.workers[0].phase,
        WorkerPhase::Reserved,
        "缺编号和 A 的结果都不能改到 B"
    );
    assert_eq!(
        store
            .record_event("ws", &finished(Some("d-b"), Outcome::Failed))
            .expect("finish b"),
        TaskEventResult::Recorded {
            task_id: "b".into(),
            wake: true,
        },
        "d-b 必须落到任务 B"
    );
    assert_eq!(
        store.get("ws", "a").expect("a again").workers[0].phase,
        WorkerPhase::Finished(WorkerOutcome::Succeeded),
        "B 的失败不能改写 A"
    );
    assert_eq!(
        store.get("ws", "b").expect("b again").workers[0].phase,
        WorkerPhase::Finished(WorkerOutcome::Failed),
        "B 只记录自己的失败"
    );
}
