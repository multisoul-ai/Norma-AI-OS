use super::*;
use crate::{AgentTaskState, AgentTaskUpdate, Runtime};
use canvas::{AgentProfile, CanvasRect};
use terminal::PtyCommand;

fn bounds() -> CanvasRect {
    CanvasRect::new(0.0, 0.0, 640.0, 400.0)
}

fn update(node_id: NodeId, to: AgentTaskState, provider_event: &str) -> AgentTaskUpdate {
    AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Codex,
        provider_event: provider_event.to_string(),
        to,
        task_title: None,
        session_id: None,
    }
}

/// 手动中断后的旧 Hook 不得重新激活 Agent；只有下一条用户 prompt 才能开始新任务。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node_id=1，由 `spawn_agent_with_command` 创建。
///   第一轮 UserPromptSubmit → Thinking，表示运行特效已经启动。
///   ManualInterrupt → Ended，表示用户已按 Ctrl+C；随后 PostToolUse 是可能晚到的旧 Hook。
///   第二轮 UserPromptSubmit 是用户明确发起的新任务，允许从 Ended 重新进入 Thinking。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入 Thinking，再写入 ManualInterrupt/Ended。
///   2. 尝试写入晚到的 PostToolUse/Thinking。
///   3. 写入下一轮 UserPromptSubmit/Thinking。
///
/// 预期结果：
///   - 正断言：ManualInterrupt 后状态为 Ended。
///   - 正断言：晚到的 PostToolUse 被忽略，状态仍为 Ended。
///   - 正断言：下一轮 UserPromptSubmit 可以恢复 Thinking。
///   - 负断言：旧 PostToolUse 不得让状态恢复 Thinking，否则运行特效会复活。
///   - 负断言：新 UserPromptSubmit 不得继续停在 Ended，避免用户无法开始下一轮任务。
#[test]
fn test_manual_interrupt_ignores_late_hook_until_next_user_prompt() {
    let mut runtime = Runtime::new();
    let node_id = runtime
        .spawn_agent_with_command(
            AgentProfile::Codex,
            bounds(),
            PtyCommand::new("/bin/sh").args(["-lc", "exit 0"]),
            80,
            24,
        )
        .expect("测试 Agent 替身应可成功 spawn");
    runtime.take_events();

    assert!(
        runtime.apply_agent_task_update(update(
            node_id,
            AgentTaskState::Thinking,
            "UserPromptSubmit"
        )),
        "第一轮 UserPromptSubmit 应启动 Agent 任务"
    );
    assert!(
        runtime.apply_agent_task_update(update(node_id, AgentTaskState::Ended, "ManualInterrupt")),
        "ManualInterrupt 应立即结束当前 Agent 任务"
    );
    let late_hook_changed =
        runtime.apply_agent_task_update(update(node_id, AgentTaskState::Thinking, "PostToolUse"));
    let state_after_late_hook = runtime.agent_task_state(node_id);
    let next_prompt_changed = runtime.apply_agent_task_update(update(
        node_id,
        AgentTaskState::Thinking,
        "UserPromptSubmit",
    ));
    let state_after_next_prompt = runtime.agent_task_state(node_id);

    assert!(
        !late_hook_changed,
        "ManualInterrupt 后的旧 PostToolUse 必须被忽略，不能重新激活任务"
    );
    assert_eq!(
        state_after_late_hook,
        Some(AgentTaskState::Ended),
        "晚到 Hook 后必须保留 Ended，避免 cursor 特效重新运行"
    );
    assert_ne!(
        state_after_late_hook,
        Some(AgentTaskState::Thinking),
        "旧 PostToolUse 不得把已中断 Agent 恢复为 Thinking"
    );
    assert!(
        next_prompt_changed,
        "下一轮 UserPromptSubmit 应允许重新开始任务"
    );
    assert_eq!(
        state_after_next_prompt,
        Some(AgentTaskState::Thinking),
        "新 prompt 后 Agent 应恢复 Thinking"
    );
    assert_ne!(
        state_after_next_prompt,
        Some(AgentTaskState::Ended),
        "新 prompt 后 Agent 不得继续停在 Ended"
    );

    runtime.kill_all();
}
