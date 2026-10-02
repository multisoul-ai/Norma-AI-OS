use agent::{
    AgentAssignment, AgentRosterEntry, AgentTaskCancellation, AgentTaskCoordinator,
    AgentTrackingMode,
};
use norma::protocol::AgentProfile;

fn coordinator() -> AgentTaskCoordinator {
    AgentTaskCoordinator::new(vec![AgentRosterEntry {
        definition_id: 1,
        name: "Alan".into(),
        speech_alias: None,
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(10),
        tracking: AgentTrackingMode::Managed,
        busy: false,
        task_state: agent::AgentTaskState::Ready,
    }])
    .expect("合法名册")
}

/// 提前交给 CLI 的任务只保留结果跟踪，不得在前一任务结束后再次派发或假装撤回。
///
/// 数据构造：Alan 一张卡片，任务 first 和 second 共 2 项；逐一记录真实 Enter 成功。
/// 执行过程：
///   1. 首任务完成输入传输后立即取出 second，不等待 first 终态。
///   2. 记录 second 送达，尝试撤回 second，再完成 first。
///   3. 查询两个任务状态，完成 second 并确认无剩余输入。
///
/// 预期结果：
///   - second 的 task_id 保持不变；first 结束后 second 成为当前任务。
///   - 已送达消息不出现在 Norma 待发送队列，不能本地撤回，也不得重发。
#[test]
fn cli_delivery_preserves_tracking_without_resubmission_or_fake_cancellation() {
    let mut coordinator = coordinator();
    let first = coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "first")], false)
        .expect("首任务")
        .remove(0);
    let second = coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "second")], false)
        .expect("第二任务")
        .remove(0);
    coordinator.mark_task_submitted(first.task_id);
    assert_eq!(
        coordinator
            .next_unsubmitted(1)
            .expect("运行中应可发送下一项")
            .task_id,
        second.task_id,
        "必须继续发送 second"
    );
    coordinator.mark_task_submitted(second.task_id);
    assert!(
        coordinator.next_unsubmitted(1).is_none(),
        "送达后不得再次返回待发送正文"
    );
    assert_eq!(
        coordinator.queued_task_count(1),
        0,
        "CLI 已接收的消息不应显示在 Norma 待发送队列"
    );
    assert!(
        coordinator
            .queue_snapshot(1)
            .expect("Alan 快照")
            .waiting
            .is_empty(),
        "不得保留可本地撤回的假队列项"
    );
    assert_eq!(
        coordinator.cancel_task(second.task_id),
        AgentTaskCancellation::AlreadySubmitted,
        "本地无法撤回 CLI 中的消息"
    );
    assert!(
        coordinator.finish_current(1).is_none(),
        "first 完成后不能重发已交给 CLI 的 second"
    );
    assert_eq!(
        coordinator
            .task_by_ref(&first.task_ref)
            .expect("first 跟踪")
            .state,
        "succeeded",
        "first 应单独完成"
    );
    assert_eq!(
        coordinator
            .task_by_ref(&second.task_ref)
            .expect("second 跟踪")
            .state,
        "running",
        "second 不能被误取消或随 first 完成"
    );
    assert_eq!(
        coordinator
            .queue_snapshot(1)
            .expect("Alan 快照")
            .current
            .expect("当前任务")
            .task_id,
        second.task_id,
        "结果必须继续关联 second 的原编号"
    );
    assert!(
        coordinator.finish_current(1).is_none(),
        "second 完成后无下一项"
    );
    assert!(!coordinator.is_busy(1), "全部完成后应解除跟踪状态");
}

/// 取消正在传输但尚未 Enter 的第二任务后，第三任务不等待首任务结束。
///
/// 数据构造：first 已提交，second/third 未提交；共 3 项，取消 1 项后保留 2 项。
/// 执行过程：记录 first 送达，取消 second 的待提交输入，读取下一次传输回执。
/// 预期结果：second 为 cancelled，first 仍 running；下一项必须是 third，不能重发 first。
#[test]
fn cancelling_unsubmitted_followup_continues_delivery_while_first_runs() {
    let mut coordinator = coordinator();
    let receipts = coordinator
        .dispatch(
            vec![
                AgentAssignment::new("Alan", "first"),
                AgentAssignment::new("Alan", "second"),
                AgentAssignment::new("Alan", "third"),
            ],
            false,
        )
        .expect("三个任务");
    coordinator.mark_task_submitted(receipts[0].task_id);
    let (cancelled, next) = coordinator.cancel_unsubmitted_task(receipts[1].task_id);
    assert_eq!(
        cancelled,
        AgentTaskCancellation::Cancelled,
        "尚未送达的 second 应可取消"
    );
    assert_eq!(
        next.expect("取消输入后应继续传输").task_id,
        receipts[2].task_id,
        "必须继续发送 third，不能等待 first 完成"
    );
    assert_eq!(
        coordinator
            .task_by_ref(&receipts[0].task_ref)
            .expect("first")
            .state,
        "running",
        "首任务不应被取消或提前完成"
    );
    assert_eq!(
        coordinator
            .task_by_ref(&receipts[1].task_ref)
            .expect("second")
            .state,
        "cancelled",
        "second 必须标记取消，不能仍处于 queued"
    );
}
