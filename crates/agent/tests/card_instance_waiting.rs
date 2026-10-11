use agent::card_instance::{CardInstanceAssignment, CardInstanceCoordinator, CardInstanceTracking};
use agent::workspace_dispatch::{CardExecutionFingerprint, WorkspaceCardRef, WorkspaceCardTarget};

/// 同一queued任务的等待原因变化必须产生新的回执证据，而不是只记录首次WaitingInput。
/// 数据构造：精确实例managed assignment，先等启动再等审批再等输入占用。
/// 执行过程：accept后连续调用waiting，并重复相同原因，读取events和最新receipt。
/// 预期结果：三次变化各一事件，重复原因不额外发事件，最新waiting_reason准确保留。
#[test]
fn waiting_reason_changes_are_observable_without_duplicate_unchanged_events() {
    let mut coordinator = CardInstanceCoordinator::default();
    let assignment = CardInstanceAssignment {
        assignment_id: "assignment-a".into(),
        target: WorkspaceCardTarget {
            card: WorkspaceCardRef {
                workspace_id: "alpha".into(),
                area_id: "area-a".into(),
                node_id: 1,
                card_instance_id: "card-a".into(),
            },
            execution: CardExecutionFingerprint {
                runtime_instance_id: "runtime-a".into(),
                route_generation: 1,
                terminal_session_id: Some(1),
                run_id: Some("run-a".into()),
                session_id: Some("session-a".into()),
            },
        },
        name_at_acceptance: "Alan".into(),
        definition_id: None,
        text: "修复登录".into(),
        tracking: CardInstanceTracking::Managed,
        force_untracked: false,
    };
    coordinator.accept(assignment).expect("应接收实例");
    coordinator.take_events();
    for reason in [
        "startup_pending",
        "approval_pending",
        "input_owned",
        "input_owned",
    ] {
        coordinator
            .waiting("assignment-a", reason)
            .expect("合法等待应更新");
    }
    let events = coordinator.take_events();
    assert_eq!(events.len(), 3, "三次原因变化必须可观察，重复原因不得刷屏");
    assert_eq!(
        events[1].receipt.waiting_reason.as_deref(),
        Some("approval_pending"),
        "历史事件应保留当时等待原因"
    );
    assert_eq!(
        coordinator
            .receipt("assignment-a")
            .expect("回执存在")
            .waiting_reason
            .as_deref(),
        Some("input_owned"),
        "当前回执应保留最新真实等待原因"
    );
}

/// Runtime停止只能证明排队任务未输入，不能证明已提交任务的真实Provider终态。
/// 数据构造：同一卡片两个不同assignment，A仍accepted、B已submitted。
/// 执行过程：interrupt_unfinished后读取两条回执，不恢复任何pending输入。
/// 预期结果：A Interrupted且retry_safe，B OutcomeUnknown且不可盲重试；pending为空。
#[test]
fn stopped_runtime_distinguishes_unsent_and_submitted_outcomes() {
    let mut coordinator = CardInstanceCoordinator::default();
    let target = WorkspaceCardTarget {
        card: WorkspaceCardRef {
            workspace_id: "alpha".into(),
            area_id: "area-a".into(),
            node_id: 1,
            card_instance_id: "card-a".into(),
        },
        execution: CardExecutionFingerprint {
            runtime_instance_id: "runtime-a".into(),
            route_generation: 1,
            terminal_session_id: Some(1),
            run_id: Some("run-a".into()),
            session_id: Some("session-a".into()),
        },
    };
    for id in ["a", "b"] {
        coordinator
            .accept(CardInstanceAssignment {
                assignment_id: id.into(),
                target: target.clone(),
                name_at_acceptance: "Alan".into(),
                definition_id: None,
                text: format!("任务{id}"),
                tracking: CardInstanceTracking::Managed,
                force_untracked: false,
            })
            .expect("应接收两条原任务");
    }
    coordinator.submitted("b").expect("B有Enter证据");
    coordinator.interrupt_unfinished();
    let a = coordinator.receipt("a").expect("A原回执存在");
    let b = coordinator.receipt("b").expect("B原回执存在");
    assert_eq!(
        a.stage,
        agent::card_instance::CardInstanceStage::Interrupted,
        "未输入A应中断"
    );
    assert!(
        a.failure.expect("中断须保留零输入资格").retry_safe,
        "A允许用户手动安全重试"
    );
    assert_eq!(
        b.stage,
        agent::card_instance::CardInstanceStage::OutcomeUnknown,
        "停止Runtime不能伪造B终态"
    );
    assert!(
        !b.failure.expect("未知须有原因").retry_safe,
        "B禁止盲目重发"
    );
    assert!(
        coordinator.pending_assignments().is_empty(),
        "停止后不能恢复旧输入队列"
    );
}

/// 原Hook先证实终态，可信结果读取稍后到达时可补原回合结果且不重新结束任务。
/// 数据构造：managed A提交并绑定turn-a，成功终态无结果；随后exactturn有结果，wrongturn和改终态请求作反例。
/// 执行过程：observe_terminal先成功后补结果，再重复补结果、尝试失败与wrongturn。
/// 预期结果：补结果只一次provider_result_snapshot，终态保留Succeeded且不会归给别的turn。
#[test]
fn exact_result_after_terminal_enriches_live_receipt_once() {
    use agent::card_instance::{CardInstanceStage, CardInstanceTurn};
    let mut coordinator = CardInstanceCoordinator::default();
    let target = WorkspaceCardTarget {
        card: WorkspaceCardRef {
            workspace_id: "alpha".into(),
            area_id: "area-a".into(),
            node_id: 1,
            card_instance_id: "card-a".into(),
        },
        execution: CardExecutionFingerprint {
            runtime_instance_id: "runtime-a".into(),
            route_generation: 1,
            terminal_session_id: Some(1),
            run_id: Some("run-a".into()),
            session_id: Some("session-a".into()),
        },
    };
    coordinator
        .accept(CardInstanceAssignment {
            assignment_id: "a".into(),
            target: target.clone(),
            name_at_acceptance: "Alan".into(),
            definition_id: None,
            text: "任务a".into(),
            tracking: CardInstanceTracking::Managed,
            force_untracked: false,
        })
        .expect("应接收A");
    coordinator.submitted("a").expect("Enter确认");
    let turn = CardInstanceTurn {
        run_id: Some("run-a".into()),
        session_id: "session-a".into(),
        turn_id: "turn-a".into(),
    };
    assert!(
        coordinator.observe_start(&target, "任务a", turn.clone()),
        "原turn应绑定A"
    );
    assert!(
        coordinator.observe_terminal(&target, &turn, CardInstanceStage::Succeeded, None),
        "Hook首次应结束原turn"
    );
    coordinator.take_events();
    let result: norma::protocol::AgentTaskResult = serde_json::from_value(
        serde_json::json!({"availability":"available","text":"修复完成","truncated":false}),
    )
    .expect("可信摘要fixture");
    assert!(
        coordinator.observe_terminal(
            &target,
            &turn,
            CardInstanceStage::Succeeded,
            Some(result.clone())
        ),
        "原turn晚到结果应补回执"
    );
    assert!(
        !coordinator.observe_terminal(
            &target,
            &turn,
            CardInstanceStage::Succeeded,
            Some(result.clone())
        ),
        "相同结果不能重复发事件"
    );
    assert!(
        !coordinator.observe_terminal(
            &target,
            &turn,
            CardInstanceStage::Failed,
            Some(result.clone())
        ),
        "补结果不能改变成功终态"
    );
    let mut wrong = turn;
    wrong.turn_id = "other-turn".into();
    assert!(
        !coordinator.observe_terminal(
            &target,
            &wrong,
            CardInstanceStage::Succeeded,
            Some(result.clone())
        ),
        "错误turn不能补结果"
    );
    let receipt = coordinator.receipt("a").expect("原回执存在");
    assert_eq!(
        receipt.stage,
        CardInstanceStage::Succeeded,
        "原终态不能回退"
    );
    assert_eq!(receipt.result, Some(result), "原结果应保留");
    let events = coordinator.take_events();
    assert_eq!(events.len(), 1, "仅首次enrichment发一事件");
    assert_eq!(
        events[0].evidence, "provider_result_snapshot",
        "新摘要使用明确结果证据"
    );
}
