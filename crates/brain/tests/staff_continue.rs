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
        model: None,
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
                model: None,
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

/// 场景：用户这一轮发话但没有点名。开关关闭时仍要问；开关打开且无风险时直接续派，不占事件额度。
/// 数据构造：AgentC 的 Worker 结果为 Unavailable；用户触发的回合进行中。先 auto=false，再换一份 auto=true。
/// 预期结果：关闭时原因是 autonomy_ask，提议仍指向原 Worker；打开时已批准，计数保持 0，不新增 Worker。
#[test]
fn unnamed_user_turn_continue_follows_the_switch() {
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    let (task, outcome) = store
        .assign(
            "ws",
            continue_op("AgentC", "d-1", "Finish the snake"),
            false,
            false,
        )
        .expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Proposed {
            proposal_id: 2,
            reason: ProposeReason::AutonomyAsk
        },
        "没点名且开关关闭，用户轮也要问"
    );
    let proposal = task.proposals.iter().find(|p| p.id == 2).expect("proposal");
    assert_eq!(
        proposal.target_worker.as_deref(),
        Some("d-1"),
        "提议必须指向要续做的 Worker"
    );
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    let (task, outcome) = store
        .assign(
            "ws",
            continue_op("AgentC", "d-1", "Finish the snake"),
            true,
            false,
        )
        .expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 2 },
        "开关打开且无风险时直接续派"
    );
    assert_eq!(task.auto_dispatches, 0, "用户轮不计入自动派发次数");
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
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::Event)
        .expect("turn");
    let (_, outcome) = store
        .assign(
            "ws",
            continue_op("AgentC", "d-1", "Finish the snake"),
            true,
            false,
        )
        .expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 2 },
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
            false,
        )
        .expect("assign");
    assert!(
        matches!(&outcome, AssignOutcome::Proposed { reason: ProposeReason::Risk(r), .. } if r.contains(&RiskCategory::Release)),
        "check: outcome {outcome:?}"
    );
}

/// 场景：目标 Worker 不能被续派。
/// 数据构造：仍在执行的 Worker；不存在的 dispatch；Agent 与 Worker 不一致；同一个 Worker 已有待采用的续派提议。
/// 预期结果：各自返回结构化错误码，且不留下新的提议。
#[test]
fn unusable_targets_are_refused_without_leaving_a_proposal() {
    let refused = |store: &TaskStore, op| {
        store
            .assign("ws", op, true, false)
            .expect_err("worker must be refused")
            .to_string()
    };
    let (_dir, store) = store_with_worker(None);
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    assert_eq!(
        refused(&store, continue_op("AgentC", "d-1", "busy")),
        "staff.worker_busy"
    );
    assert_eq!(
        refused(&store, continue_op("AgentC", "nope", "unknown")),
        "staff.worker_unknown"
    );
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Succeeded));
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    assert_eq!(
        refused(&store, continue_op("AgentA", "d-1", "wrong agent")),
        "staff.worker_agent_mismatch"
    );
    assert_eq!(
        store.get("ws", "a").expect("task").proposals.len(),
        1,
        "被拒绝的续派不留提议"
    );
    // 第一条续派（设置为询问且事件回合）降级成待采用提议后，同一 Worker 不能再接第二条。
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::Event)
        .expect("turn");
    store
        .assign("ws", continue_op("AgentC", "d-1", "first"), false, false)
        .expect("first continue");
    assert_eq!(
        refused(&store, continue_op("AgentC", "d-1", "second")),
        "staff.worker_pending"
    );
}

/// 场景：事件触发的回合（Worker 完成事件）自动续派不能无限循环。
/// 数据构造：auto=true、事件回合；连续续派并绑定 5 次后第 6 次；用户回合里把计数设成 5。
/// 预期结果：事件回合的前 5 次直接派发并计数，第 6 次因次数上限降级；用户回合不受上限影响。
#[test]
fn event_turn_continues_are_capped_but_user_turns_are_not() {
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::Event)
        .expect("turn");
    for round in 0..5u64 {
        let (task, outcome) = store
            .assign("ws", continue_op("AgentC", "d-1", "again"), true, false)
            .expect("assign");
        assert!(
            matches!(outcome, AssignOutcome::Dispatch { .. }),
            "round {round}: {outcome:?}"
        );
        assert_eq!(
            task.auto_dispatches,
            round as u32 + 1,
            "事件回合的续派计入次数"
        );
        let id = task.proposals.last().expect("proposal").id;
        store.rebind_worker("ws", "a", id, "p").expect("rebind");
        store
            .update("ws", "a", |task| {
                task.finish_worker(10, "d-1", WorkerOutcome::Unavailable, "still nothing");
                Ok(())
            })
            .expect("finish");
    }
    let (_, outcome) = store
        .assign("ws", continue_op("AgentC", "d-1", "again"), true, false)
        .expect("assign");
    assert!(
        matches!(
            outcome,
            AssignOutcome::Proposed {
                reason: ProposeReason::LimitReached,
                ..
            }
        ),
        "check: outcome {outcome:?}"
    );
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .update("ws", "a", |task| {
            task.auto_dispatches = 5;
            Ok(())
        })
        .expect("cap reached");
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    let (_, outcome) = store
        .assign(
            "ws",
            continue_op("AgentC", "d-1", "user says go"),
            true,
            false,
        )
        .expect("assign");
    assert!(
        matches!(outcome, AssignOutcome::Dispatch { .. }),
        "用户回合不受上限影响：{outcome:?}"
    );
}

/// 场景：降级成提议的续派，用户采用后仍续派同一个 Worker；采用时改了 Agent 则拒绝。
/// 数据构造：事件回合 auto=false 得到未批准的提议 2；approve 后 rebind；另一份里 approve 时换成 AgentA。
/// 预期结果：采用后提议仍带 target_worker，rebind 复用原 Worker；换 Agent 的 rebind 报 Agent 不一致。
#[test]
fn adopted_continue_proposal_still_targets_the_worker() {
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::Event)
        .expect("turn");
    store
        .assign("ws", continue_op("AgentC", "d-1", "go on"), false, false)
        .expect("assign");
    store.approve("ws", "a", 2, None, None).expect("approve");
    let task = store.get("ws", "a").expect("task");
    let proposal = task.proposals.iter().find(|p| p.id == 2).expect("proposal");
    assert_eq!(
        proposal.target_worker.as_deref(),
        Some("d-1"),
        "采用后仍指向原 Worker"
    );
    let task = store.rebind_worker("ws", "a", 2, "p").expect("rebind");
    assert_eq!(task.workers.len(), 1, "采用后续派不新增 Worker");

    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Unavailable));
    store
        .begin_turn("ws", "a", ReplyTrigger::Event)
        .expect("turn");
    store
        .assign("ws", continue_op("AgentC", "d-1", "go on"), false, false)
        .expect("assign");
    store
        .approve("ws", "a", 2, Some("AgentA".into()), None)
        .expect("approve");
    let error = store
        .rebind_worker("ws", "a", 2, "p")
        .expect_err("agent changed");
    assert_eq!(
        error.to_string(),
        "staff.worker_agent_mismatch",
        "check: error"
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
            true,
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

/// [场景描述]：续派用 engine 标识而不是 Worker 的名字，预检通过后重绑也必须成功。
///
/// 数据构造：
///   Worker 名字 AgentC，engine OpenCode，dispatch d-1，已经结束
///   续派的 agent 字段写 "opencode"（小写 engine），不是 "AgentC"
///
/// 执行过程：
///   1. assign 用 is_agent 认 engine，生成已批准的续派提议
///   2. rebind 必须用同一规则，不能再拿 "AgentC" != "opencode" 拒绝
///
/// 预期结果：
///   - rebind 成功，仍是原来那张卡和 d-1
///   - 无关名字的拒绝仍由既有用例覆盖（approve 改成 AgentA 后 rebind 返回 staff.worker_agent_mismatch）
#[test]
fn rebind_accepts_the_worker_engine() {
    let (_dir, store) = store_with_worker(Some(WorkerOutcome::Succeeded));
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("turn");
    store
        .assign(
            "ws",
            continue_op("opencode", "d-1", "Finish the snake"),
            true,
            false,
        )
        .expect("assign");
    let task = store
        .rebind_worker("ws", "a", 2, "handoff text")
        .expect("engine id must rebind the same worker");
    assert_eq!(
        task.workers[0].dispatch_id, "d-1",
        "engine 续派不能换 dispatch"
    );
    assert_eq!(task.workers[0].card_id, Some(10), "engine 续派不能换卡片");
}
