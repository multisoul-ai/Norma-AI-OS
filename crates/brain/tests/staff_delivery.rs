use brain::staff::{TaskEventResult, TaskStore};
use brain::{BrainEventKind, DeliveryPhase};
use norma::protocol::{StaffOp, Task, Worker, WorkerChip, WorkerState};

fn delivery(node: u64, dispatch: &str, phase: DeliveryPhase) -> BrainEventKind {
    BrainEventKind::WorkerDelivery {
        node_id: node,
        dispatch_id: dispatch.into(),
        phase,
    }
}

/// 未确认送达显示启动中；需要你之后同一提议不再新建 Worker，重新投递仍是原卡。
///
/// 数据构造：任务 ship，Agent Alan，node=7，dispatch=d-1，prompt 为 "build it"。
/// title "Build" 与 why "Needed" 合计 11 个字符，低于 50 的提议上限。
/// 执行过程：绑定 → NeedsAttention 两次 → 再提同一 prompt 和另一 prompt → Redeliver → Accepted。
/// 预期结果：绑定后 Waiting/Starting；需要你只加一条消息且仍是一张卡；同一 prompt 不再提议；
/// 另一 prompt 可以提议；重投不换 card_id；送达后才 Running。
#[test]
fn needs_attention_redelivers_the_same_card() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    store
        .create(Task::new("ship", "ws", "project", "Ship"))
        .expect("create");
    store
        .execute(
            "ws",
            StaffOp::Propose {
                task_id: "ship".into(),
                agent: "Alan".into(),
                title: "Build".into(),
                prompt: "build it".into(),
                why: "Needed".into(),
            },
        )
        .expect("propose");
    store.approve("ws", "ship", 1).expect("approve");
    let mut worker = Worker::new("Alan", "OpenCode", "Build", 7, "d-1");
    worker.launch_prompt = "wrapped build it".into();
    store.bind_worker("ws", "ship", 1, worker).expect("bind");
    let bound = store.get("ws", "ship").expect("bound");
    assert_eq!(
        bound.display_status(),
        norma::protocol::TaskDisplayStatus::Waiting,
        "a freshly bound Worker has not received the prompt"
    );
    assert_eq!(
        bound.workers[0].chip(),
        WorkerChip::Starting,
        "the chip stays on starting until delivery is confirmed"
    );
    assert_eq!(
        store
            .record_event("ws", &delivery(7, "d-1", DeliveryPhase::NeedsAttention))
            .expect("attention"),
        TaskEventResult::Recorded {
            task_id: "ship".into(),
            wake: false,
        },
        "the first timeout is recorded without waking the model"
    );
    assert_eq!(
        store
            .record_event("ws", &delivery(7, "d-1", DeliveryPhase::NeedsAttention))
            .expect("repeat"),
        TaskEventResult::Ignored,
        "the same timeout does not append another message"
    );
    let attention = store.get("ws", "ship").expect("attention task");
    assert_eq!(
        attention.display_status(),
        norma::protocol::TaskDisplayStatus::NeedsYou,
        "a delivery timeout needs the user"
    );
    assert_eq!(
        attention.workers[0].chip(),
        WorkerChip::NeedsYou,
        "the chip uses the needs-you state"
    );
    assert_eq!(attention.workers.len(), 1, "timeout must not add a worker");
    assert_eq!(
        attention
            .messages
            .iter()
            .filter(|message| {
                matches!(
                    message,
                    norma::protocol::TaskMessage::WorkerNeedsAttention { .. }
                )
            })
            .count(),
        1,
        "one dispatch gets one needs-attention message"
    );
    store
        .execute(
            "ws",
            StaffOp::Propose {
                task_id: "ship".into(),
                agent: "Alan".into(),
                title: "Again".into(),
                prompt: "build it".into(),
                why: "Retry".into(),
            },
        )
        .expect("same proposal");
    assert_eq!(
        store.get("ws", "ship").expect("same").proposals.len(),
        1,
        "NeedsAttention keeps the original proposal"
    );
    store
        .execute(
            "ws",
            StaffOp::Propose {
                task_id: "ship".into(),
                agent: "Alan".into(),
                title: "Other".into(),
                prompt: "write docs".into(),
                why: "Separate".into(),
            },
        )
        .expect("other proposal");
    assert_eq!(
        store.get("ws", "ship").expect("other").proposals.len(),
        2,
        "a different prompt can still be proposed"
    );
    store
        .record_event("ws", &delivery(7, "d-1", DeliveryPhase::Redeliver))
        .expect("redeliver");
    let retried = store.get("ws", "ship").expect("retried");
    assert_eq!(
        retried.workers.len(),
        1,
        "redelivery must not create a card"
    );
    assert_eq!(
        retried.workers[0].card_id,
        Some(7),
        "redelivery stays on the original card"
    );
    assert_eq!(
        retried.workers[0].dispatch_id, "d-1",
        "redelivery keeps the original dispatch"
    );
    assert_eq!(
        retried.workers[0].state,
        WorkerState::Active,
        "redelivery returns the worker to active"
    );
    assert!(
        !retried.workers[0].prompt_delivered,
        "redelivery waits for a new UserPromptSubmit"
    );
    assert_eq!(
        retried.workers[0].launch_prompt, "wrapped build it",
        "the wrapped prompt remains available for the same card"
    );
    store
        .record_event("ws", &delivery(7, "d-1", DeliveryPhase::Accepted))
        .expect("accepted");
    let running = store.get("ws", "ship").expect("running");
    assert!(
        running.workers[0].prompt_delivered,
        "UserPromptSubmit marks the prompt delivered"
    );
    assert_eq!(
        running.workers[0].chip(),
        WorkerChip::Running,
        "the chip becomes running only after delivery"
    );
    assert_ne!(
        running.workers[0].state,
        WorkerState::NeedsAttention,
        "acceptance clears the needs-attention state"
    );
}

/// 升级前正在执行的 Worker 快照没有送达字段，仍应显示执行中。
///
/// 数据构造：旧 JSON 不含 prompt_delivered 与 launch_prompt。
/// 执行过程：反序列化成 Worker。
/// 预期结果：prompt_delivered 为 true，launch_prompt 为空；新 Worker 则相反。
#[test]
fn old_worker_snapshot_stays_delivered() {
    let worker: Worker = serde_json::from_str(
        r#"{"agent":"Alan","engine":"OpenCode","role":"Build","card_id":7,"dispatch_id":"d","engine_session_id":null,"state":"Active","outcome":null}"#,
    )
    .expect("old snapshot");
    assert!(
        worker.prompt_delivered,
        "a snapshot written before delivery tracking is already in progress"
    );
    assert!(
        worker.launch_prompt.is_empty(),
        "a missing launch prompt falls back to the proposal"
    );
    let created = Worker::new("Alan", "OpenCode", "Build", 7, "d");
    assert!(
        !created.prompt_delivered,
        "a newly reserved Worker has not been delivered"
    );
    assert_ne!(
        created.chip(),
        WorkerChip::Running,
        "a new Worker must not look like it is already running"
    );
}
