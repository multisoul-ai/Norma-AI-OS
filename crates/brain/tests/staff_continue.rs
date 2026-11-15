use brain::staff::policy::{ProposeReason, RiskCategory};
use brain::staff::{AssignOutcome, TaskStore};
use norma::protocol::{ReplyTrigger, StaffOp, Task, Worker, WorkerOutcome};

fn continue_op(agent: &str, worker: &str, prompt: &str) -> StaffOp {
    StaffOp::Assign {
        task_id: "a".into(),
        agent: agent.into(),
        title: "Keep going".into(),
        prompt: prompt.into(),
        why: "Unfinished".into(),
        worker: Some(worker.into()),
    }
}

/// 任务里 AgentC 的 Worker（卡片 10、dispatch d-1）已经结束，结果为 `outcome`；`None` 表示仍在执行。
fn store_with_worker(outcome: Option<WorkerOutcome>) -> (tempfile::TempDir, TaskStore) {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().to_owned());
    store
        .create(Task::new("a", "ws", "project", "Snake"))
        .expect("create");
    store
        .execute(
            "ws",
            StaffOp::Propose {
                task_id: "a".into(),
                agent: "AgentC".into(),
                title: "Build".into(),
                prompt: "Build".into(),
                why: "Required".into(),
            },
        )
        .expect("propose");
    store.approve("ws", "a", 1, None, None).expect("approve");
    store
        .bind_worker(
            "ws",
            "a",
            1,
            Worker::new("AgentC", "OpenCode", "Build", 10, "d-1"),
        )
        .expect("bind");
    if let Some(outcome) = outcome {
        store
            .update("ws", "a", |task| {
                task.finish_worker(10, "d-1", outcome, "nothing trusted");
                Ok(())
            })
            .expect("finish");
    }
    (dir, store)
}

/// 场景：用户这一轮发话要求继续，幕僚把活交回没有可信结果的 AgentC，设置为「询问」。
/// 数据构造：AgentC 的 Worker 结果为 Unavailable；用户触发的回合进行中；auto=false。
/// 预期结果：直接续派，提议指向同一个 Worker，不计入自动次数，也没有新增 Worker。
#[test]
fn user_turn_continues_a_resultless_worker_without_asking() {
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    let (task, outcome) = store
        .assign(
            "ws",
            continue_op("AgentC", "d-1", "Finish the snake"),
            false,
        )
        .expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 2 },
        "check: outcome"
    );
    let proposal = task.proposals.iter().find(|p| p.id == 2).expect("proposal");
    assert_eq!(
        proposal.target_worker.as_deref(),
        Some("d-1"),
        "提议必须指向要续做的 Worker"
    );
    assert!(proposal.approved, "用户这一轮已发话，续派不再确认");
    assert_eq!(task.auto_dispatches, 0, "续派不计入自动派发次数");
    assert_eq!(task.workers.len(), 1, "续派不新增 Worker");
}

/// 场景：没有用户发话的回合（例如 Worker 完成事件触发），幕僚自己想续做。
/// 数据构造：事件触发的回合进行中；分别用 auto=false 和 auto=true。
/// 预期结果：设置为询问时降级为提议（原因是设置）；设置为自动时直接续派。
#[test]
fn event_turn_follows_the_followup_setting() {
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::Event)
        .expect("turn");
    let (task, outcome) = store
        .assign(
            "ws",
            continue_op("AgentC", "d-1", "Finish the snake"),
            false,
        )
        .expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Proposed {
            proposal_id: 2,
            reason: ProposeReason::AutonomyAsk
        },
        "check: outcome when asking"
    );
    let proposal = task.proposals.iter().find(|p| p.id == 2).expect("proposal");
    assert_eq!(
        proposal.target_worker.as_deref(),
        Some("d-1"),
        "降级后的提议仍要指向同一个 Worker"
    );
    assert!(!proposal.approved, "降级的提议等用户采用");
    let (_, outcome) = store
        .assign("ws", continue_op("AgentC", "d-1", "Finish the snake"), true)
        .expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 3 },
        "check: outcome when automatic"
    );
}

/// 场景：用户发话要求继续，但内容含风险动作。
/// 数据构造：用户回合进行中；prompt 要求 push 到远端；auto=true。
/// 预期结果：仍降级为提议，原因是风险。
#[test]
fn risky_continue_is_still_downgraded_in_a_user_turn() {
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    let (_, outcome) = store
        .assign(
            "ws",
            continue_op("AgentC", "d-1", "Finish it then git push --force"),
            true,
        )
        .expect("assign");
    assert!(
        matches!(&outcome, AssignOutcome::Proposed { reason: ProposeReason::Risk(r), .. } if r.contains(&RiskCategory::Release)),
        "check: outcome {outcome:?}"
    );
}

/// 场景：目标 Worker 不能被续派。
/// 数据构造：仍在执行的 Worker；不存在的 dispatch；Agent 与 Worker 不一致。
/// 预期结果：三种都报错，且不留下任何提议。
#[test]
fn unusable_targets_are_refused_without_leaving_a_proposal() {
    let (_dir, store) = store_with_worker(None);
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    for op in [
        continue_op("AgentC", "d-1", "busy worker"),
        continue_op("AgentC", "nope", "unknown worker"),
    ] {
        assert!(
            store.assign("ws", op, true).is_err(),
            "worker must be refused"
        );
    }
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Succeeded));
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    assert!(
        store
            .assign("ws", continue_op("AgentA", "d-1", "wrong agent"), true)
            .is_err(),
        "agent must match the worker"
    );
    assert_eq!(
        store.get("ws", "a").expect("task").proposals.len(),
        1,
        "被拒绝的续派不留提议"
    );
}

/// 场景：宿主确认卡片还在后，把续派绑定到原 Worker。
/// 数据构造：直接续派得到的提议 2。
/// 预期结果：同一个 Worker 结果清空回到执行中，卡片和 dispatch 都不变（进程环境里带着它），Worker 数量不变，提议记下该 dispatch。
#[test]
fn rebind_reuses_the_worker_card_and_dispatch() {
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    store
        .assign(
            "ws",
            continue_op("AgentC", "d-1", "Finish the snake"),
            false,
        )
        .expect("assign");
    let task = store
        .rebind_worker("ws", "a", 2, "handoff text")
        .expect("rebind");
    let worker = &task.workers[0];
    assert_eq!(task.workers.len(), 1, "check: workers");
    assert_eq!(
        worker.dispatch_id, "d-1",
        "dispatch 必须不变，hook 才会被接受"
    );
    assert_eq!(worker.card_id, Some(10), "必须是同一张卡片");
    assert_eq!(worker.outcome, None, "续派后回到执行中");
    assert!(!worker.prompt_delivered, "新 prompt 还没送达");
    assert_eq!(worker.launch_prompt, "handoff text", "check: launch prompt");
    let proposal = task.proposals.iter().find(|p| p.id == 2).expect("proposal");
    assert_eq!(
        proposal.dispatch_id.as_deref(),
        Some("d-1"),
        "check: proposal dispatch"
    );
    assert!(
        store.rebind_worker("ws", "a", 2, "again").is_err(),
        "同一条提议不能重复续派"
    );
}
