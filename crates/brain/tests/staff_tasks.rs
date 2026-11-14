use brain::staff::TaskEventResult;
use brain::staff::TaskStore;
use brain::{BrainEventKind, Outcome};
use norma::protocol::{Task, TaskPhase};
use norma::protocol::{TaskDisplayStatus, Worker, WorkerOutcome, WorkerState};

/// 手动阶段在保存和重开后受到保护，新消息重新打开已完成任务。
///
/// 数据构造：工作空间 ws-a，任务 task-a；阶段 Active → Done；消息数 0 → 2（重开事件 + 用户消息）。
/// 执行过程：1. 建任务并存盘。2. 用户设 Done，自动设 Merging。3. 重载并发新消息。
/// 预期结果：自动推进不能覆盖 Done；新消息后 Active，历史含重开事件，磁盘与内存一致。
#[test]
fn manual_phase_survives_reload_and_message_reopens_done() {
    let dir = tempfile::tempdir().expect("temporary store");
    let store = TaskStore::new(dir.path().to_owned());
    store
        .create(Task::new("task-a", "ws-a", "project-a", "Ship staff"))
        .expect("create task");
    store
        .update("ws-a", "task-a", |task| {
            task.set_phase(TaskPhase::Done, true);
            task.set_phase(TaskPhase::Merging, false);
            Ok(())
        })
        .expect("save phase");
    let reopened = TaskStore::new(dir.path().to_owned());
    let saved = reopened.get("ws-a", "task-a").expect("reload task");
    assert_eq!(
        saved.phase,
        TaskPhase::Done,
        "automatic phase must not replace the user's Done"
    );
    assert!(
        saved.phase_manual,
        "manual phase ownership must survive disk reload"
    );
    assert!(
        reopened.get("ws-b", "task-a").is_err(),
        "another workspace must not see this task"
    );
    reopened
        .update("ws-a", "task-a", |task| {
            task.user_message("Continue");
            Ok(())
        })
        .expect("new message");
    let task = reopened.get("ws-a", "task-a").expect("read reopened task");
    assert_eq!(
        task.phase,
        TaskPhase::Active,
        "new user input must reopen Done"
    );
    assert!(
        !task.phase_manual,
        "reopening starts a new automatic phase cycle"
    );
    assert_eq!(
        task.messages.len(),
        2,
        "reopen event and user message must both be retained"
    );
    assert_eq!(
        TaskStore::new(dir.path().to_owned())
            .get("ws-a", "task-a")
            .expect("reload again"),
        task,
        "disk must match the published task"
    );
}

/// 持久化事件后才允许唤醒；中断静默，失败立即处理，跨工作空间与重复事件拒绝。
///
/// 数据构造：同名 Simon 在 ws-a/node21 和 ws-b/node22，各有一个 dispatch；超时不参与。
/// 执行过程：1. 中断 A。2. 重复中断 A。3. B 失败。4. 从磁盘重载检查原消息。
/// 预期结果：中断有终态且 wake=true；重复 Ignored；失败 wake=true；两个任务各仅一条消息。
#[test]
fn events_are_durable_scoped_and_interrupted_wakes() {
    let dir = tempfile::tempdir().expect("store root");
    let store = TaskStore::new(dir.path().to_owned());
    let mut a = Task::new("a", "ws-a", "project", "A");
    let mut b = Task::new("b", "ws-b", "project", "B");
    a.workers
        .push(Worker::new("Simon", "Codex", "Build", 21, "d-a"));
    b.workers
        .push(Worker::new("Simon", "Codex", "Build", 22, "d-b"));
    store.create(a).expect("save A");
    store.create(b).expect("save B");
    let interrupted = BrainEventKind::AgentFinished {
        node_id: 21,
        dispatch_id: Some("d-a".into()),
        agent: "Simon".into(),
        outcome: Outcome::Interrupted,
        excerpt: "interrupted".into(),
        from_result: false,
    };
    assert_eq!(
        store
            .record_event("ws-a", &interrupted)
            .expect("record interruption"),
        TaskEventResult::Recorded {
            task_id: "a".into(),
            wake: true
        },
        "interruption must persist and wake the task"
    );
    assert_eq!(
        store.record_event("ws-a", &interrupted).expect("duplicate"),
        TaskEventResult::Ignored,
        "duplicate event must not wake or report twice"
    );
    let failed = BrainEventKind::AgentFinished {
        node_id: 22,
        dispatch_id: Some("d-b".into()),
        agent: "Simon".into(),
        outcome: Outcome::Failed,
        excerpt: "failed".into(),
        from_result: true,
    };
    assert_eq!(
        store
            .record_event("ws-a", &failed)
            .expect("wrong workspace"),
        TaskEventResult::Unowned,
        "same name must not route B into A"
    );
    assert_eq!(
        store.record_event("ws-b", &failed).expect("record failure"),
        TaskEventResult::Recorded {
            task_id: "b".into(),
            wake: true
        },
        "failure must wake after commit"
    );
    let loaded = TaskStore::new(dir.path().to_owned());
    let a = loaded.get("ws-a", "a").expect("reload A");
    let b = loaded.get("ws-b", "b").expect("reload B");
    assert_eq!(
        a.workers[0].state,
        WorkerState::Interrupted,
        "A must retain interruption"
    );
    assert_eq!(a.messages.len(), 1, "A must contain only its interruption");
    assert_eq!(b.messages.len(), 1, "B must contain only its failure");
    assert_ne!(
        a.workers[0].state,
        WorkerState::Failed,
        "B failure must not alter A"
    );
}

/// 结果必须匹配节点和派发编号；正常完成进入待合并，失败或缺失结果转人工。
///
/// 数据构造：任务 A/B 各一 Worker，名字同为 Simon，node 分别 11/12，dispatch 分别 d-a/d-b。
/// 执行过程：1. 向 A 交付 B 的节点和旧编号。2. 交付 A 真结果并重复。3. B 交付无可信结果。
/// 预期结果：未送达是等待而不是执行中；错误归属不写消息；成功只写一次并自动 Merging；B 保持 Active 且需要你。
#[test]
fn result_identity_deduplication_and_missing_result_are_explicit() {
    let mut a = Task::new("a", "ws-a", "project", "A");
    let mut b = Task::new("b", "ws-a", "project", "B");
    a.workers
        .push(Worker::new("Simon", "Codex", "Implement", 11, "d-a"));
    b.workers
        .push(Worker::new("Simon", "Codex", "Check", 12, "d-b"));
    assert_eq!(
        a.display_status(),
        TaskDisplayStatus::Waiting,
        "an undelivered Worker is not running yet"
    );
    assert_eq!(
        a.workers[0].chip(),
        norma::protocol::WorkerChip::Starting,
        "the card is still starting until UserPromptSubmit"
    );
    a.workers[0].prompt_delivered = true;
    assert_eq!(
        a.display_status(),
        TaskDisplayStatus::Running,
        "delivery confirmation is what makes the Worker running"
    );
    assert!(
        !a.finish_worker(12, "d-a", WorkerOutcome::Succeeded, "wrong node"),
        "same Agent name must not route another node"
    );
    assert!(
        !a.finish_worker(11, "old", WorkerOutcome::Succeeded, "stale"),
        "stale dispatch must not settle current Worker"
    );
    assert!(
        a.messages.is_empty(),
        "rejected events must not become task messages"
    );
    assert!(
        a.finish_worker(11, "d-a", WorkerOutcome::Succeeded, "implemented"),
        "matching result must be accepted"
    );
    assert!(
        !a.finish_worker(11, "d-a", WorkerOutcome::Succeeded, "duplicate"),
        "same terminal event must be idempotent"
    );
    assert_eq!(
        a.messages.len(),
        1,
        "exactly one result message must survive"
    );
    assert_eq!(
        a.phase,
        TaskPhase::Merging,
        "all results ready must automatically advance to review"
    );
    assert!(
        b.finish_worker(
            12,
            "d-b",
            WorkerOutcome::Unavailable,
            "user prompt fallback"
        ),
        "missing trusted result must be recorded"
    );
    assert_eq!(
        b.phase,
        TaskPhase::Active,
        "untrusted fallback must not complete task"
    );
    assert_eq!(
        b.display_status(),
        TaskDisplayStatus::NeedsYou,
        "missing result requires user attention"
    );
    assert_ne!(
        b.workers[0].outcome,
        Some(WorkerOutcome::Succeeded),
        "fallback is not a successful result"
    );
    let mut failed = Task::new("f", "ws-a", "project", "Failure");
    failed
        .workers
        .push(Worker::new("Simon", "Codex", "Check", 13, "d-f"));
    assert!(
        failed.finish_worker(13, "d-f", WorkerOutcome::Failed, "failed"),
        "failure must settle"
    );
    assert_eq!(
        failed.workers[0].state,
        WorkerState::Failed,
        "worker must expose failure"
    );
    assert_eq!(
        failed.display_status(),
        TaskDisplayStatus::NeedsYou,
        "failure requires a decision"
    );
    assert_ne!(
        failed.phase,
        TaskPhase::Merging,
        "failure must not be represented as ready for review"
    );
}

/// Worker 等待用户审批时展示需要你，恢复后回到运行中；旧派发不能改状态。
/// 数据构造：一任务一 Worker，node=30、dispatch=current，尚无结果。
/// 执行过程：等待审批 → 旧 dispatch 解除等待（拒绝）→ 当前 dispatch 解除等待。
/// 预期结果：状态为 NeedsYou → NeedsYou → Running；摘要明确需审批 Worker；不是终态，不自动完成。
#[test]
fn waiting_for_user_is_task_attention_without_settling_worker() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().into());
    let mut task = Task::new("waiting", "ws", "p", "Approve");
    let mut worker = Worker::new("Alan", "Claude", "Build", 30, "current");
    worker.prompt_delivered = true;
    task.workers.push(worker);
    store.create(task).expect("create");
    let attention = |dispatch: &str, waiting| BrainEventKind::WorkerAttention {
        node_id: 30,
        dispatch_id: dispatch.into(),
        waiting,
    };
    store
        .record_event("ws", &attention("current", true))
        .expect("waiting");
    assert_eq!(
        store.get("ws", "waiting").expect("task").display_status(),
        TaskDisplayStatus::NeedsYou,
        "unanswered approval must surface in task list"
    );
    assert_eq!(
        store
            .record_event("ws", &attention("old", false))
            .expect("old"),
        TaskEventResult::Ignored,
        "old dispatch cannot clear attention"
    );
    let waiting_summary: serde_json::Value = serde_json::from_str(
        &store
            .summary_for("ws")
            .expect("summary")
            .expect("task summary"),
    )
    .expect("JSON");
    assert_eq!(
        waiting_summary["needs_you"],
        serde_json::json!(["Alan"]),
        "summary must name the Worker waiting for approval"
    );
    store
        .record_event("ws", &attention("current", false))
        .expect("resumed");
    let resumed_summary: serde_json::Value = serde_json::from_str(
        &store
            .summary_for("ws")
            .expect("summary")
            .expect("task summary"),
    )
    .expect("JSON");
    assert_eq!(
        resumed_summary["needs_you"],
        serde_json::json!([]),
        "resumed Worker must no longer appear in attention list"
    );
    let task = store.get("ws", "waiting").expect("resumed task");
    assert_eq!(
        task.display_status(),
        TaskDisplayStatus::Running,
        "accepted answer restores running state"
    );
    assert!(
        task.workers[0].outcome.is_none(),
        "waiting/resuming must not settle the Worker"
    );
    assert_ne!(
        task.phase,
        TaskPhase::Done,
        "approval is not task completion"
    );
}

/// 预期结果：不含关键词的普通成功也会唤醒幕僚（triage 只管播报，不管唤醒）。
#[test]
fn plain_success_wakes_the_task() {
    let dir = tempfile::tempdir().expect("store root");
    let store = TaskStore::new(dir.path().to_owned());
    let mut task = Task::new("a", "ws-a", "project", "A");
    task.workers
        .push(Worker::new("Simon", "Codex", "Build", 21, "d-a"));
    store.create(task).expect("save");
    let done = BrainEventKind::AgentFinished {
        node_id: 21,
        dispatch_id: Some("d-a".into()),
        agent: "Simon".into(),
        outcome: Outcome::Succeeded,
        excerpt: "已完成".into(),
        from_result: true,
    };
    assert_eq!(
        store.record_event("ws-a", &done).expect("record"),
        TaskEventResult::Recorded {
            task_id: "a".into(),
            wake: true
        },
    );
}
