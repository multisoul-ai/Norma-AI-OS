use agent::{
    AgentAssignment, AgentDispatchSource, AgentRosterEntry, AgentTaskCoordinator, AgentTrackingMode,
};
use norma::protocol::AgentProfile;

fn roster(definition_id: i64, name: &str) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id,
        name: name.to_string(),
        speech_alias: None,
        profile: AgentProfile::Claude,
        enabled: true,
        active_node_id: Some(definition_id as u64 + 100),
        tracking: AgentTrackingMode::Managed,
        busy: false,
        task_state: agent::AgentTaskState::Ready,
    }
}

fn task_state(coordinator: &AgentTaskCoordinator, task_ref: &str) -> String {
    coordinator
        .task_by_ref(task_ref)
        .map(|task| task.state.to_string())
        .unwrap_or_else(|_| "missing".to_string())
}

/// 队列或语音的手动派发也是 Manual 且没有 parent，但不能被下一条用户 prompt 提前收掉。
///
/// 数据构造：
///   Alan definition_id=1，node=101。
///   手动派发 1 条，来源 `AgentDispatchSource::manual()`，parent 为空。
///
/// 执行过程：
///   1. 把「检查边界」手动派发给空闲的 Alan，任务立刻成为 active。
///   2. 模拟卡片发出 UserPromptSubmit，调用 register_external_root。
///
/// 预期结果：
///   - 正断言：created=false，返回的仍是这条手动派发，状态保持 running。
///   - 负断言：它不得变成 succeeded，否则调用方会看到任务刚提交就完成。
#[test]
fn user_prompt_does_not_replace_manual_dispatch() {
    let mut coordinator = AgentTaskCoordinator::new_with_runtime_id(
        "runtime-keep-manual-dispatch",
        vec![roster(1, "Alan")],
    )
    .expect("单 Agent roster 应建立协调器");
    let dispatched = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Alan", "检查边界")],
            false,
            AgentDispatchSource::manual(),
        )
        .expect("空闲 Agent 的手动派发应立即成为 active")
        .remove(0);
    let (kept, created) = coordinator
        .register_external_root(1, 101, "卡片里的用户回合")
        .expect("查询手动派发不应失败");

    assert!(
        !created,
        "手动派发的 active 不得被 UserPromptSubmit 换成外部根"
    );
    assert_eq!(
        kept.task_ref, dispatched.task_ref,
        "拒绝覆盖时必须返回仍在执行的手动派发"
    );
    assert_eq!(
        task_state(&coordinator, &dispatched.task_ref),
        "running",
        "手动派发必须继续 running"
    );
    assert_ne!(
        task_state(&coordinator, &dispatched.task_ref),
        "succeeded",
        "手动派发不得在回合刚开始时被标成 succeeded"
    );
}
