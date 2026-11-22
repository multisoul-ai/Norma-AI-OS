use super::{assign_op, store_with_finished_worker};
use brain::staff::AssignOutcome;
use norma::protocol::{ReplyTrigger, StaffOp, TaskMessage, Worker, WorkerOutcome, WorkerPhase};

const PROMPT: &str = "等 CI 通过再合并。\nCo-Authored-By: Example <noreply@example.com>";

fn assign_original_owner(agent: &str, prior_title: &str) -> StaffOp {
    let mut op = assign_op(agent, PROMPT);
    if let StaffOp::Assign { owner_of, .. } = &mut op {
        *owner_of = Some(prior_title.into());
    }
    op
}

/// 场景：用户要求原负责人合并改动，但没有叫出名字。
/// 数据构造：Simon 已收到并完成提议 Build；用户轮未启用自动派发。
/// 执行过程：先指定一个不匹配的 Agent，再用 Simon 及同一条历史提议派发。
/// 预期结果：不匹配时停在选人、没有新提议；验证到唯一负责人后直接派发并保留原始正文。
#[test]
fn original_owner_is_verified_before_dispatch_without_another_approval() {
    let (_dir, store) = store_with_finished_worker("Simon");
    store
        .update("ws", "a", |task| {
            task.user_message("让原负责人把改动通过 PR 合入 dev");
            Ok(())
        })
        .expect("authorize follow-up");
    let before = store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("user turn");
    let error = store
        .assign("ws", assign_original_owner("Grace", "Build"), false, false)
        .expect_err("a different agent cannot be used as the original owner");
    assert_eq!(
        error.to_string(),
        "staff.owner_unresolved",
        "ask the user to select a recipient"
    );
    assert_eq!(
        store.get("ws", "a").expect("task"),
        before,
        "rejection must not create a proposal"
    );

    let (task, outcome) = store
        .assign("ws", assign_original_owner("Simon", "Build"), false, false)
        .expect("assign to original owner");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 2 },
        "known recipient needs no further confirmation"
    );
    let proposal = &task.proposals[1];
    assert_eq!(
        proposal.agent, "Simon",
        "send to the recipient from the earlier dispatch"
    );
    assert_eq!(
        proposal.prompt, PROMPT,
        "preserve the complete follow-up instructions"
    );
    assert_eq!(
        proposal.downgrade, None,
        "no confirmation card for this recipient"
    );
}

/// 场景：同一事项的两个 Worker 都有收件记录，另一个事项还没有收件记录。
/// 数据构造：Build 最先派给 Simon，再派给 Grace；Review 只有待采用提议。
/// 执行过程：在用户轮分别尝试把这两项交给 Simon。
/// 预期结果：两项都需要选人，已有提议和 Worker 记录不发生变化。
#[test]
fn unverified_or_conflicting_owners_leave_dispatch_state_untouched() {
    let (_dir, store) = store_with_finished_worker("Simon");
    for (title, prompt) in [
        ("Build", "Finish the build"),
        ("Review", "Check the changes"),
    ] {
        store
            .execute(
                "ws",
                StaffOp::Propose {
                    task_id: "a".into(),
                    agent: "Grace".into(),
                    title: title.into(),
                    prompt: prompt.into(),
                    why: "Prior work".into(),
                    model: None,
                },
            )
            .expect("record prior work");
    }
    store
        .approve("ws", "a", 2, None, None)
        .expect("approve second build");
    store
        .bind_worker(
            "ws",
            "a",
            2,
            Worker::new("Grace", "Codex", "Build", 20, "d-2"),
        )
        .expect("bind second build");
    store
        .update("ws", "a", |task| {
            task.finish_worker(20, "d-2", WorkerOutcome::Succeeded, "received and finished");
            Ok(())
        })
        .expect("record result");
    let before = store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("user turn");
    for title in ["Build", "Review"] {
        let error = store
            .assign("ws", assign_original_owner("Simon", title), true, false)
            .expect_err("ambiguous or missing recipient must stop before dispatch");
        assert_eq!(
            error.to_string(),
            "staff.owner_unresolved",
            "the host must request a recipient for {title}"
        );
    }
    assert_eq!(
        store.get("ws", "a").expect("task"),
        before,
        "neither lookup may change the task or dispatch anything"
    );
}

/// 场景：上次派发只绑定了卡片，进程未启动，Worker 没有收件。
/// 数据构造：历史事项的 Worker 在启动时失败，宿主留下失败结果。
/// 执行过程：在用户轮以该历史事项为依据派给上次尝试的 Worker。
/// 预期结果：失败的启动不能证明接收人，任务状态保持不变。
#[test]
fn a_failed_launch_does_not_prove_the_worker_received_the_item() {
    let (_dir, store) = store_with_finished_worker("Simon");
    store
        .update("ws", "a", |task| {
            task.messages
                .retain(|message| !matches!(message, TaskMessage::WorkerFinished { .. }));
            task.received_proposals.clear();
            task.workers[0].phase = WorkerPhase::Spawning;
            task.finish_worker(10, "d-1", WorkerOutcome::Failed, "launch failed");
            Ok(())
        })
        .expect("record failed launch");
    let before = store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("user turn");
    let error = store
        .assign("ws", assign_original_owner("Simon", "Build"), true, false)
        .expect_err("launch failure does not confirm recipient");
    assert_eq!(
        error.to_string(),
        "staff.owner_unresolved",
        "only verified delivery can route a follow-up"
    );
    assert_eq!(
        store.get("ws", "a").expect("task"),
        before,
        "the follow-up must remain unassigned"
    );
}

/// 场景：用户询问进度，或要求审批前先看一眼。
/// 数据构造：同一事项有唯一原负责人，两条原话都没有授权立即发送。
/// 执行过程：让幕僚尝试引用该原负责人并直接派发。
/// 预期结果：两次都不产生提议或 Worker；原负责人不能代替授权。
#[test]
fn showing_status_or_draft_does_not_authorize_an_owner_assignment() {
    for user in [
        "原负责人现在做到哪了？",
        "让原负责人合并的方案先给我看",
        "之前指派给谁了？",
        "之前派给 Iris 的任务进度怎么样？",
    ] {
        let (_dir, store) = store_with_finished_worker("Simon");
        store
            .update("ws", "a", |task| {
                task.user_message(user);
                Ok(())
            })
            .expect("record user request");
        let before = store
            .begin_turn("ws", "a", ReplyTrigger::User)
            .expect("user turn");
        let error = store
            .assign("ws", assign_original_owner("Simon", "Build"), true, false)
            .expect_err("a prior recipient does not authorize new work");
        assert_eq!(
            error.to_string(),
            "staff.owner_assignment_not_requested",
            "direct dispatch requires an authorized request"
        );
        assert_eq!(
            store.get("ws", "a").expect("task"),
            before,
            "no proposal or worker created"
        );
    }
}

/// 场景：旧 Worker 已确认收件，但同一张卡开始另一条续派，名册也存在同名重建的可能。
/// 数据构造：ID 7 的 Simon 完成 Build，再在同卡续派 Keep working，等待收件。
/// 执行过程：用户授权原负责人续接时，分别用旧 ID 和重建后的 ID 查这两条提议。
/// 预期结果：只有原名册身份和已确认收件的 Build 可以确定负责人。
#[test]
fn receipts_stay_on_the_original_item_and_roster_identity() {
    let (_dir, store) = store_with_finished_worker("Simon");
    store
        .update("ws", "a", |task| {
            task.workers[0].roster_id = Some(7);
            Ok(())
        })
        .expect("record historical roster identity");
    let mut next = assign_op("Simon", "Keep working");
    if let StaffOp::Assign { worker, .. } = &mut next {
        *worker = Some("d-1".into());
    }
    store
        .assign("ws", next, true, false)
        .expect("assign more work to same Worker");
    store
        .rebind_worker("ws", "a", 2, "Keep working")
        .expect("stage another delivery");
    store
        .update("ws", "a", |task| {
            task.user_message("请原负责人跟进");
            Ok(())
        })
        .expect("record request");
    store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("user turn");
    for (title, id) in [("Build", 9), ("Next", 7)] {
        let error = store
            .assign_for_recipient(
                "ws",
                assign_original_owner("Simon", title),
                false,
                false,
                Some((id, "Codex")),
            )
            .expect_err("same name or same card does not prove a recipient");
        assert_eq!(
            error.to_string(),
            "staff.owner_unresolved",
            "mismatched or unreceived item must wait for user choice"
        );
    }
    let (_, outcome) = store
        .assign_for_recipient(
            "ws",
            assign_original_owner("Simon", "Build"),
            false,
            false,
            Some((7, "Codex")),
        )
        .expect("previous receipt survives continuing the card");
    assert_eq!(
        outcome,
        AssignOutcome::Dispatch { proposal_id: 3 },
        "the prior receipt still verifies the current roster member"
    );
}
