use super::*;
use runtime::AgentTaskState;

/// Agent 失败、完成或中断后，即使 PTY 尚未退出，聚焦光标也必须停止推进动画相位。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node 的 NodeRunState=Running，表示交互式 shell 仍存活
///   Thinking 表示当前任务在执行，应保持 cursor 动画
///   Failed/Done/Ended 分别表示任务失败、正常结束、会话结束，应停止 cursor 动画
///   Term node 的 NodeRunState=Running 用作对照，普通终端不受 Agent task state 影响
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 Agent + Thinking 解析聚焦 cursor 动画意图
///   2. 用同一个仍为 Running 的 Agent 分别代入 Failed、Done、Ended
///   3. 用普通 Terminal + Running 验证 PTY 生命周期行为保持不变
///
/// 预期结果：
///   - 正断言：Thinking 时动画启用
///   - 正断言：Failed、Done、Ended 时动画关闭
///   - 正断言：普通运行中 Terminal 仍启用动画
///   - 负断言：Failed Agent 不得因为 PTY 存活而继续动画
#[test]
fn test_agent_terminal_cursor_animation_stops_when_task_ends_before_pty_exits() {
    let agent_running = Some(runtime::NodeRunState::Running);
    let thinking = focused_cursor_animation_active(
        canvas::NodeType::Agent,
        agent_running.clone(),
        Some(AgentTaskState::Thinking),
    );
    let failed = focused_cursor_animation_active(
        canvas::NodeType::Agent,
        agent_running.clone(),
        Some(AgentTaskState::Failed {
            message: "tool failed".to_string(),
        }),
    );
    let done = focused_cursor_animation_active(
        canvas::NodeType::Agent,
        agent_running.clone(),
        Some(AgentTaskState::Done),
    );
    let ended = focused_cursor_animation_active(
        canvas::NodeType::Agent,
        agent_running,
        Some(AgentTaskState::Ended),
    );
    let terminal_running = focused_cursor_animation_active(
        canvas::NodeType::Term,
        Some(runtime::NodeRunState::Running),
        None,
    );

    assert!(thinking, "Thinking Agent 应继续推进 cursor 动画");
    assert!(
        !failed,
        "Failed Agent 必须停止 cursor 动画，不能沿用仍存活 PTY 的状态"
    );
    assert!(!done, "Done Agent 必须停止 cursor 动画");
    assert!(!ended, "Ended Agent 必须停止 cursor 动画");
    assert!(
        terminal_running,
        "普通仍在运行的 Terminal 应继续使用 cursor 动画"
    );
    assert_ne!(
        failed, terminal_running,
        "Failed Agent 与运行中 Terminal 的 cursor 动画意图必须不同"
    );
}
