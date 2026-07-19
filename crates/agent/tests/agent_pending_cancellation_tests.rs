use agent::{
    AgentAssignment, AgentDispatchState, AgentRosterEntry, AgentTaskCoordinator, AgentTrackingMode,
};
use norma::protocol::AgentProfile;

fn idle_alan() -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id: 1,
        name: "Alan".to_string(),
        speech_alias: None,
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(101),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    }
}

/// AC-6：尚未按 Enter 提交的 active task 取消后必须直接终止，并提升下一项等待任务。
///
/// 数据构造（含关键数值的推导过程）：
///   Alan 初始 busy=false；dispatch tasks=2
///   task 1 = active 但尚未提交，task 2 = queued
///   cancel operations=1（只取消 task 1）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 连续派发两项，使第一项占用 active、第二项进入 FIFO
///   2. 对第一项调用 cancel_unsubmitted_task
///   3. 检查取消终态、提升回执和协调器当前任务
///
/// 预期结果：
///   - 正断言：第一项立即 Cancelled，第二项提升为 SubmittedManaged
///   - 负断言：第一项不再是 running/cancel_requested，队列中不残留第二项副本
#[test]
fn coordinator_cancels_unsubmitted_active_task_and_promotes_next() {
    let mut coordinator =
        AgentTaskCoordinator::new(vec![idle_alan()]).expect("空闲 Alan roster 应能建立协调器");
    let first = coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "第一项")], false)
        .expect("第一项应占用 active 槽位")[0]
        .clone();
    let second = coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "第二项")], false)
        .expect("第二项应进入 FIFO")[0]
        .clone();

    let (cancelled, promoted) = coordinator.cancel_unsubmitted_task(first.task_id);
    let promoted = promoted.expect("取消未提交第一项后应提升第二项");
    let tasks = coordinator.tasks();

    assert_eq!(
        cancelled,
        agent::AgentTaskCancellation::Cancelled,
        "未提交第一项必须立即进入 Cancelled"
    );
    assert_eq!(
        promoted.task_id, second.task_id,
        "提升回执必须对应原 FIFO 第二项"
    );
    assert_eq!(
        promoted.state,
        AgentDispatchState::SubmittedManaged,
        "提升的第二项必须成为 managed active task"
    );
    assert_eq!(
        coordinator.queued_task_count(1),
        0,
        "第二项提升后 FIFO 不得残留重复副本"
    );
    assert!(coordinator.is_busy(1), "第二项提升后 Alan 仍应处于忙碌状态");
    assert!(
        tasks
            .iter()
            .any(|task| task.task_id == first.task_id && task.state == "cancelled"),
        "第一项必须保留可查询的 cancelled 终态"
    );
    assert!(
        !tasks
            .iter()
            .any(|task| task.task_id == first.task_id && task.state == "running"),
        "第一项不得继续显示为 running"
    );
    assert!(
        !tasks
            .iter()
            .any(|task| task.task_id == first.task_id && task.state == "cancel_requested"),
        "从未提交的第一项不应进入 cancel_requested 中间态"
    );
}
