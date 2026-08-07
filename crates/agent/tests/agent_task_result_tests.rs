use agent::{
    AgentAssignment, AgentRosterEntry, AgentTaskCoordinator, AgentTaskState, AgentTrackingMode,
};
use norma::protocol::{AgentProfile, AgentTaskResult, AgentTaskResultAvailability};

fn roster_entry(definition_id: i64, name: &str) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id,
        name: name.to_string(),
        speech_alias: None,
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(definition_id as u64 + 100),
        tracking: AgentTrackingMode::Managed,
        busy: false,
        task_state: AgentTaskState::Ready,
    }
}

/// COORD-RESULT-1：可信 Stop 结果必须与当前 task_ref 一起进入终态历史。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent = Alan(definition_id=1, node_id=101)
///   tasks = 2：task-1 当前运行，task-2 等待
///   task-1 result = "修复完成"，4 个字符 < 1200 字符上限 → truncated=false
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 连续派发两个任务，固定 task-1/task-2 的独立 task_ref。
///   2. 以 succeeded + available result 完成当前 task-1。
///   3. 协调器释放 task-2，再按 task-1 的 task_ref 查询终态历史。
///
/// 预期结果：
///   - 正断言：task-1 保留精确结果正文、available 与 truncated=false。
///   - 正断言：释放的下一项仍是 task-2，结果不会串到下一任务。
///   - 负断言：task-2 在尚未完成时不得提前拥有 task-1 的结果。
#[test]
fn coordinator_binds_terminal_result_to_exact_task_ref() {
    let mut coordinator = AgentTaskCoordinator::new(vec![roster_entry(1, "Alan")])
        .expect("单个 managed Agent 应能建立协调器");
    let receipts = coordinator
        .dispatch(
            vec![
                AgentAssignment::new("Alan", "first task"),
                AgentAssignment::new("Alan", "second task"),
            ],
            false,
        )
        .expect("两个任务应形成 active + queued FIFO");
    let result = AgentTaskResult {
        availability: AgentTaskResultAvailability::Available,
        text: Some("修复完成".to_string()),
        truncated: false,
    };

    let released = coordinator
        .finish_current_with_result(1, "succeeded", Some(result.clone()))
        .expect("完成 task-1 后必须释放 task-2");
    let completed = coordinator
        .task_by_ref(&receipts[0].task_ref)
        .expect("task-1 终态历史必须可按 task_ref 查询");
    let still_running = coordinator
        .task_by_ref(&receipts[1].task_ref)
        .expect("task-2 必须继续作为当前任务存在");

    assert_eq!(
        completed.result,
        Some(result),
        "task-1 必须保留同一 Stop 事件捕获的精确结果"
    );
    assert_eq!(
        released.task_ref, receipts[1].task_ref,
        "完成 task-1 后只能释放 FIFO 中的 task-2"
    );
    assert_eq!(
        still_running.result, None,
        "尚未完成的 task-2 不得继承 task-1 的结果"
    );
    assert_ne!(
        completed.task_ref, still_running.task_ref,
        "两个任务必须继续以不同 task_ref 隔离结果"
    );
}
