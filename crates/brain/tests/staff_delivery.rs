use brain::staff::{TaskEventResult, TaskStore};
use brain::{BrainEventKind, DeliveryPhase};
use norma::protocol::{StaffOp, Task, Worker, WorkerChip, WorkerPhase};

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
                model: None,
            },
        )
        .expect("propose");
    store.approve("ws", "ship", 1, None, None).expect("approve");
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
                model: None,
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
                model: None,
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
        retried.workers[0].phase,
        WorkerPhase::Spawning,
        "redelivery waits to type the prompt again"
    );
    assert!(
        !retried.workers[0].delivered(),
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
        running.workers[0].delivered(),
        "UserPromptSubmit marks the prompt delivered"
    );
    assert_eq!(
        running.workers[0].chip(),
        WorkerChip::Running,
        "the chip becomes running only after delivery"
    );
    assert_eq!(
        running.workers[0].phase,
        WorkerPhase::Running,
        "acceptance clears the unconfirmed failure"
    );
}

/// 送达、芯片和结果都从 phase 派生。v2 整份快照已备份丢弃，不再给旧字段补默认值。
///
/// 数据构造：新建 Worker，以及 Running / Waiting / Finished / Failed 四种阶段。
/// 执行过程：读 delivered、chip、outcome。
/// 预期结果：新建是启动中且未送达；Running 才是执行中；成功结束是完成；失败、中断、未确认都是需要你。
#[test]
fn phase_derives_delivery_chip_and_outcome() {
    let created = Worker::new("Alan", "OpenCode", "Build", 7, "d");
    assert_eq!(
        created.phase,
        WorkerPhase::Reserved,
        "a new card is reserved"
    );
    assert!(
        !created.delivered(),
        "a newly reserved Worker has not been delivered"
    );
    assert_eq!(
        created.chip(),
        WorkerChip::Starting,
        "a new Worker must not look like it is already running"
    );
    assert!(
        created.launch_prompt.is_empty(),
        "launch prompt starts empty"
    );
    let mut running = created.clone();
    running.phase = WorkerPhase::Running;
    assert!(running.delivered(), "running means the prompt was accepted");
    assert_eq!(running.chip(), WorkerChip::Running, "running chip");
    assert!(running.outcome().is_none(), "running has no outcome yet");
    let mut waiting = running.clone();
    waiting.phase = WorkerPhase::Waiting;
    assert_eq!(
        waiting.chip(),
        WorkerChip::NeedsYou,
        "waiting for the user needs attention"
    );
    let mut done = running.clone();
    done.phase = WorkerPhase::Finished(norma::protocol::WorkerOutcome::Succeeded);
    assert_eq!(done.chip(), WorkerChip::Done, "success is done");
    assert!(done.delivered(), "a finished worker was delivered");
    let mut failed = running.clone();
    failed.phase = WorkerPhase::Finished(norma::protocol::WorkerOutcome::Failed);
    assert_eq!(
        failed.chip(),
        WorkerChip::NeedsYou,
        "a failed result needs the user"
    );
    let mut unconfirmed = created.clone();
    unconfirmed.phase = WorkerPhase::Failed(norma::protocol::WorkerFailure::Unconfirmed);
    assert_eq!(
        unconfirmed.chip(),
        WorkerChip::NeedsYou,
        "a delivery timeout needs the user"
    );
    assert!(
        !unconfirmed.delivered(),
        "timeout means the prompt never landed"
    );
    assert!(
        unconfirmed.outcome().is_none(),
        "timeout is not a finished outcome"
    );
}

/// 15 秒窗口后才到的确认：已标成需要你的 Worker 直接收回到执行中，不需要重新投递。
///
/// 数据构造：任务 late，Agent Alan，node=9，dispatch=d-9，提议已采用并绑定。
/// 执行过程：NeedsAttention → 不经 Redeliver 直接 Accepted → 再来一次 Accepted。
/// 预期结果：超时后芯片是需要你；晚到确认被记录并回到执行中；重复确认被忽略；仍只有一个 Worker。
#[test]
fn late_acceptance_recovers_a_needs_attention_worker() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    store
        .create(Task::new("late", "ws", "project", "Late"))
        .expect("create");
    store
        .execute(
            "ws",
            StaffOp::Propose {
                task_id: "late".into(),
                agent: "Alan".into(),
                title: "Build".into(),
                prompt: "build it".into(),
                why: "Needed".into(),
                model: None,
            },
        )
        .expect("propose");
    store.approve("ws", "late", 1, None, None).expect("approve");
    store
        .bind_worker(
            "ws",
            "late",
            1,
            Worker::new("Alan", "Claude", "Build", 9, "d-9"),
        )
        .expect("bind");
    store
        .record_event("ws", &delivery(9, "d-9", DeliveryPhase::NeedsAttention))
        .expect("attention");
    assert_eq!(
        store.get("ws", "late").expect("timed out").workers[0].chip(),
        WorkerChip::NeedsYou,
        "the timeout marks the worker as needing the user"
    );
    assert_eq!(
        store
            .record_event("ws", &delivery(9, "d-9", DeliveryPhase::Accepted))
            .expect("late accepted"),
        TaskEventResult::Recorded {
            task_id: "late".into(),
            wake: false,
        },
        "a late confirmation is recorded"
    );
    let recovered = store.get("ws", "late").expect("recovered");
    assert_eq!(
        recovered.workers[0].chip(),
        WorkerChip::Running,
        "a late confirmation takes the worker back to running"
    );
    assert_eq!(recovered.workers.len(), 1, "recovery must not add a worker");
    assert_eq!(
        store
            .record_event("ws", &delivery(9, "d-9", DeliveryPhase::Accepted))
            .expect("repeat accepted"),
        TaskEventResult::Ignored,
        "a repeated confirmation changes nothing"
    );
}

/// 场景：用户在任务跑起来之前撤回，以及确认之后再点撤回。
/// 数据构造：两个任务各绑一个 Worker。一个停在 Reserved，一个先记成 Running。
/// 执行过程：对两者各发一次撤回。
/// 预期结果：Reserved 变成中断，系统消息用撤回键，wake 为 false，自动派发次数不变。
/// Running 的撤回被忽略，没有新消息。
#[test]
fn withdraw_before_running_does_not_wake() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    let bind = |id: &str, node: u64, dispatch: &str| {
        store
            .create(Task::new(id, "ws", "project", id))
            .expect("create");
        store
            .execute(
                "ws",
                StaffOp::Propose {
                    task_id: id.into(),
                    agent: "Grace".into(),
                    title: "发布".into(),
                    prompt: "ship".into(),
                    why: "now".into(),
                    model: None,
                },
            )
            .expect("propose");
        store.approve("ws", id, 1, None, None).expect("approve");
        store
            .bind_worker(
                "ws",
                id,
                1,
                Worker::new("Grace", "Claude", "发布", node, dispatch),
            )
            .expect("bind");
    };
    bind("early", 4, "d-early");
    bind("late", 5, "d-late");
    store
        .record_event("ws", &delivery(5, "d-late", DeliveryPhase::Accepted))
        .expect("running");
    let withdrawn = store
        .record_event(
            "ws",
            &BrainEventKind::WorkerWithdrawn {
                node_id: 4,
                dispatch_id: "d-early".into(),
            },
        )
        .expect("withdraw");
    assert_eq!(
        withdrawn,
        TaskEventResult::Recorded {
            task_id: "early".into(),
            wake: false,
        },
        "撤回记下来，但不唤醒幕僚"
    );
    let early = store.get("ws", "early").expect("early");
    assert_eq!(
        early.workers[0].phase,
        WorkerPhase::Finished(norma::protocol::WorkerOutcome::Interrupted),
        "还没跑起来的 Worker 记为中断"
    );
    assert_eq!(early.auto_dispatches, 0, "撤回不增加自动派发次数");
    assert!(
        early.messages.iter().any(|message| matches!(
            message,
            norma::protocol::TaskMessage::WorkerFinished { text, .. } if text == "staff.worker_withdrawn"
        )),
        "系统消息用撤回文案的键"
    );
    assert_eq!(
        store
            .record_event(
                "ws",
                &BrainEventKind::WorkerWithdrawn {
                    node_id: 5,
                    dispatch_id: "d-late".into(),
                },
            )
            .expect("late withdraw"),
        TaskEventResult::Ignored,
        "已经跑起来的撤回被忽略"
    );
    assert_eq!(
        store.get("ws", "late").expect("late").workers[0].phase,
        WorkerPhase::Running,
        "跑起来之后保持 Running"
    );
}
