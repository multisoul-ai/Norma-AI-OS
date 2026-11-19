use brain::staff::policy::{ProposeReason, RiskCategory};
use brain::staff::{AssignOutcome, TaskStore};
use norma::protocol::{StaffOp, Task, Worker, WorkerOutcome};

fn assign_op(agent: &str, prompt: &str) -> StaffOp {
    StaffOp::Assign {
        task_id: "a".into(),
        agent: agent.into(),
        title: "Next".into(),
        prompt: prompt.into(),
        why: "Follow up".into(),
        model: None,
        worker: None,
    }
}

/// 任务里 Simon 已经完成过一次派发（用户采用），之后幕僚可以接着派。
fn store_with_finished_worker(agent: &str) -> (tempfile::TempDir, TaskStore) {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().to_owned());
    store
        .create(Task::new("a", "ws", "project", "Ship"))
        .expect("create");
    store
        .execute(
            "ws",
            StaffOp::Propose {
                task_id: "a".into(),
                agent: agent.into(),
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
            Worker::new(agent, "Codex", "Build", 10, "d-1"),
        )
        .expect("bind");
    store
        .update("ws", "a", |task| {
            task.finish_worker(10, "d-1", WorkerOutcome::Succeeded, "done");
            Ok(())
        })
        .expect("finish");
    (dir, store)
}

/// 场景：幕僚给刚完成任务的同一个 Agent 派后续活，自主权为「自动」，内容无风险。
/// 数据构造：Simon 已完成第 1 条提议；assign 给 Simon 一条普通后续。
/// 预期结果：直接派发，生成第 2 条已批准提议，自动派发计数为 1，待派发的 Worker 还没绑定。
#[test]
fn follow_up_to_the_same_agent_is_dispatched_directly() {
    let (_dir, store) = store_with_finished_worker("Simon");
    let (task, outcome) = store
        .assign(
            "ws",
            assign_op("Simon", "Add the missing test"),
            true,
            false,
        )
        .expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 2 },
        "check: outcome"
    );
    let proposal = task.proposals.iter().find(|p| p.id == 2).expect("proposal");
    assert!(proposal.approved, "direct dispatch is pre-approved");
    assert!(
        proposal.dispatch_id.is_none(),
        "host binds the worker later"
    );
    assert_eq!(task.auto_dispatches, 1, "check: task.auto_dispatches");
    assert!(proposal.auto, "直接派发的提议要标记为幕僚自动派发");
    assert_eq!(proposal.downgrade, None, "直接派发没有降级原因");
}

/// 场景：设置还在「询问」，幕僚的 assign 不能直接派发。
/// 数据构造：Simon 已完成一次派发；auto=false，assign 一条普通后续。
/// 预期结果：降级为未批准的提议，原因是设置，自动派发计数仍为 0，任务对话里出现这张建议卡。
#[test]
fn assign_is_downgraded_to_a_proposal_when_settings_ask() {
    let (_dir, store) = store_with_finished_worker("Simon");
    let (task, outcome) = store
        .assign(
            "ws",
            assign_op("Simon", "Add the missing test"),
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
        "check: outcome"
    );
    let proposal = task.proposals.iter().find(|p| p.id == 2).expect("proposal");
    assert!(!proposal.approved, "the user still has to adopt it");
    assert!(!proposal.auto, "降级的提议不是自动派发");
    assert_eq!(
        proposal.downgrade.as_deref(),
        Some("autonomy_ask"),
        "卡片要能显示为什么没有自动派发"
    );
    assert_eq!(task.auto_dispatches, 0, "check: task.auto_dispatches");
    assert!(
        task.messages
            .iter()
            .any(|m| matches!(m, norma::protocol::TaskMessage::Staff(reply)
            if reply.parts.contains(&norma::protocol::StaffPart::Proposal(2)))),
        "the downgraded proposal appears as a card"
    );
}

/// 场景：Worker 被中断不再单独挡住自动派发；事件轮只看风险、开关和次数。
/// 数据构造：Simon 的 Worker 结果改为 Interrupted；auto=true，未点名，内容无风险。
/// 预期结果：直接派发，计数变成 1。撤回后不立刻重派由「不唤醒幕僚」保证，不在这条策略里。
#[test]
fn interrupted_worker_does_not_block_an_unnamed_dispatch() {
    let (_dir, store) = store_with_finished_worker("Simon");
    store
        .update("ws", "a", |task| {
            task.workers[0].outcome = Some(WorkerOutcome::Interrupted);
            Ok(())
        })
        .expect("interrupt");
    let (task, outcome) = store
        .assign(
            "ws",
            assign_op("Simon", "Add the missing test"),
            true,
            false,
        )
        .expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 2 },
        "中断不是降级原因"
    );
    assert_eq!(task.auto_dispatches, 1, "事件轮的直接派发计入次数");
}

/// 场景：用户点名后，高风险也直接派，不看开关。
/// 数据构造：用户回合进行中；named=true；auto=false；指令含 git push。
/// 预期结果：已批准的派发，计数不加（用户轮不占事件额度）。
#[test]
fn named_user_turn_dispatches_risky_work_even_when_the_switch_is_off() {
    let (_dir, store) = store_with_finished_worker("Simon");
    store
        .begin_turn("ws", "a", norma::protocol::ReplyTrigger::User)
        .expect("turn");
    let (task, outcome) = store
        .assign(
            "ws",
            assign_op("Grace", "git push the fix to main"),
            false,
            true,
        )
        .expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 2 },
        "点名就是授权"
    );
    assert_eq!(task.auto_dispatches, 0, "用户轮不计数");
    assert!(
        task.proposals.iter().any(|p| p.id == 2 && p.approved),
        "提议应已批准"
    );
}

/// 场景：内容命中高风险关键词，即使设置为自动也只能提议。
/// 数据构造：auto=true，同一 Agent，指令含 git push。
/// 预期结果：降级为未批准提议，原因带风险类别 Release。
#[test]
fn risky_assign_is_downgraded_even_when_auto() {
    let (_dir, store) = store_with_finished_worker("Simon");
    let (task, outcome) = store
        .assign(
            "ws",
            assign_op("Simon", "git push the fix to main"),
            true,
            false,
        )
        .expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Proposed {
            proposal_id: 2,
            reason: ProposeReason::Risk(vec![RiskCategory::Release])
        },
        "check: outcome"
    );
    assert!(
        !task.proposals[1].approved,
        "check: !task.proposals[1].approved"
    );
}

/// 场景：事件轮连续自动派发，第 5 次之后转提议；用户发话清零，采用提议不清零。
/// 数据构造：上限是 5。前 5 次 assign 的 proposal id 从 2 到 6（第 1 条是夹具里已采用的提议）。
/// 第 6 次新派发的 id 是 7。然后 approve 那条提议，计数应仍为 5；再写入一条用户消息后变为 0。
/// 预期结果：前 5 次直接派发，第 6 次原因是次数上限；采用不清零，用户消息清零后又能直接派。
#[test]
fn automatic_dispatches_stop_after_five_until_the_user_speaks() {
    let (_dir, store) = store_with_finished_worker("Simon");
    for expected in 2..=6 {
        let (_, outcome) = store
            .assign(
                "ws",
                assign_op("Simon", &format!("Follow up {expected}")),
                true,
                false,
            )
            .expect("assign");
        assert_eq!(
            outcome,
            AssignOutcome::Dispatch {
                proposal_id: expected
            },
            "第 {} 次仍应直接派发",
            expected - 1
        );
    }
    let (task, outcome) = store
        .assign("ws", assign_op("Simon", "One more"), true, false)
        .expect("sixth assign");
    assert_eq!(
        outcome,
        AssignOutcome::Proposed {
            proposal_id: 7,
            reason: ProposeReason::LimitReached
        },
        "第 5 次之后转提议"
    );
    assert_eq!(task.auto_dispatches, 5, "被拦住的那次不继续加计数");
    let task = store
        .approve("ws", "a", 7, None, None)
        .expect("user adopts");
    assert_eq!(task.auto_dispatches, 5, "采用提议不清零");
    store
        .execute(
            "ws",
            StaffOp::Message {
                task_id: "a".into(),
                text: "继续".into(),
            },
        )
        .expect("user message");
    let task = store.get("ws", "a").expect("task");
    assert_eq!(task.auto_dispatches, 0, "用户发话清零");
    store
        .begin_turn("ws", "a", norma::protocol::ReplyTrigger::Event)
        .expect("event turn");
    let (_, outcome) = store
        .assign("ws", assign_op("Simon", "After the reset"), true, false)
        .expect("assign after reset");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 8 },
        "清零后又能直接派发"
    );
}

/// 场景：旧快照没有自动派发计数字段，升级后不能读失败，也不能被当成已派发过。
/// 数据构造：序列化一个任务，删掉 auto_dispatches 字段再读回。
/// 预期结果：读回成功，计数为 0。
#[test]
fn old_snapshots_without_the_counter_load_as_zero() {
    let mut value = serde_json::to_value(Task::new("a", "ws", "project", "Ship")).expect("json");
    value
        .as_object_mut()
        .expect("object")
        .remove("auto_dispatches");
    let task: Task = serde_json::from_value(value).expect("old snapshot");
    assert_eq!(task.auto_dispatches, 0, "check: task.auto_dispatches");
}

/// 场景：模型发来的 Assign 不能绕过策略，通用写入口 execute 必须拒绝它。
/// 数据构造：对已有任务直接 execute(Assign)。
/// 预期结果：返回错误，任务里没有新增提议。
#[test]
fn generic_execute_refuses_assign_so_policy_cannot_be_skipped() {
    let (_dir, store) = store_with_finished_worker("Simon");
    assert!(
        store.execute("ws", assign_op("Simon", "Anything")).is_err(),
        "check: store.execute('ws', assign_op('Simon', 'Anything')"
    );
    assert_eq!(
        store.get("ws", "a").expect("task").proposals.len(),
        1,
        "check: store.get('ws', 'a').expect('task').proposals.len("
    );
}

/// [场景描述]：幕僚用 engine 标识（而非具名智能体名）派后续活，并指定模型。
///
/// 数据构造：
///   Worker 名字 "Simon"，engine "Codex"，已完成第 1 条提议
///   assign 的 agent 字段写 "codex"（小写 engine），model = "gpt-5"
///
/// 执行过程：
///   1. assign 判断「刚完成的是否同一个」：Worker.engine 与 "codex" 忽略大小写相等 → 视为同一执行者
///   2. 无风险、自动开启、次数未超限 → 直接派发
///
/// 预期结果：
///   - 结果是 Dispatch，而不是降级为 NotFollowUp
///   - 新提议（id=2）的 model 为 Some("gpt-5")，宿主据此给卡片加 --model
#[test]
fn follow_up_by_engine_id_dispatches_and_keeps_the_model() {
    let (_dir, store) = store_with_finished_worker("Simon");
    let op = match assign_op("codex", "Add the missing test") {
        StaffOp::Assign {
            task_id,
            agent,
            title,
            prompt,
            why,
            worker,
            ..
        } => StaffOp::Assign {
            task_id,
            agent,
            title,
            prompt,
            why,
            worker,
            model: Some("gpt-5".into()),
        },
        other => other,
    };
    let (task, outcome) = store.assign("ws", op, true, false).expect("assign");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 2 },
        "engine id equal (case-insensitive) to the finished worker's engine counts as a follow-up"
    );
    let proposal = task
        .proposals
        .iter()
        .find(|p| p.id == 2)
        .expect("proposal 2");
    assert_eq!(
        proposal.model.as_deref(),
        Some("gpt-5"),
        "model chosen by staff must be stored for the host to launch with"
    );
}

/// 场景：用户采用了写 engine 名的提议，宿主按 engine 挑了具名 Agent 建 Worker。
/// 数据构造：提议 agent 为 "codex"，Worker 的 agent 为 "Codex-1"、engine 为 "Codex"。
/// 预期结果：bind_worker 成功并记录 dispatch_id，而不是报 "proposal not approved"。
#[test]
fn bind_accepts_engine_named_proposal_for_named_agent_worker() {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().to_owned());
    store
        .create(Task::new("a", "ws", "project", "Ship"))
        .expect("create");
    store
        .execute(
            "ws",
            StaffOp::Propose {
                task_id: "a".into(),
                agent: "codex".into(),
                title: "Build".into(),
                prompt: "Build".into(),
                why: "Required".into(),
                model: None,
            },
        )
        .expect("propose");
    store.approve("ws", "a", 1, None, None).expect("approve");
    let task = store
        .bind_worker(
            "ws",
            "a",
            1,
            Worker::new("Codex-1", "Codex", "Build", 10, "d-1"),
        )
        .expect("bind by engine");
    let proposal = task.proposals.iter().find(|p| p.id == 1).expect("proposal");
    assert_eq!(proposal.dispatch_id.as_deref(), Some("d-1"));
}
