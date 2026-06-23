use super::*;
use crate::{AgentTaskState, AgentTaskUpdate, RuntimeEvent};
use canvas::{AgentProfile, CanvasNodeKind, CanvasRect, PreviewSpec};

fn bounds() -> CanvasRect {
    CanvasRect::new(0.0, 0.0, 640.0, 400.0)
}

fn update(node_id: NodeId, to: AgentTaskState, provider_event: &str) -> AgentTaskUpdate {
    AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Codex,
        provider_event: provider_event.to_string(),
        to,
    }
}

/// Agent task update 应只更新目标 Agent node，并发出一条 task 状态变化事件。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node_id=1，由 spawn_agent_with_command 创建
///   初始 task state=None
///   update.to=Thinking，provider_event="UserPromptSubmit"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. spawn_agent_with_command → document 中存在 Agent node
///   2. take_events 清空 NodeRegistered
///   3. apply_agent_task_update(Thinking) → task state 写入 node_id=1
///   4. take_events 读取 AgentTaskStateChanged
///
/// 预期结果：
///   - 正断言：apply 返回 true，agent_task_state(1)=Thinking
///   - 正断言：事件为 AgentTaskStateChanged，source_event=UserPromptSubmit
///   - 负断言：不应产生 NodeRunStateChanged
#[test]
fn test_agent_task_update_changes_agent_state_and_emits_event() {
    let mut runtime = Runtime::new();
    let command = PtyCommand::new("/bin/sh").args(["-lc", "exit 0"]);
    let node_id = runtime
        .spawn_agent_with_command(AgentProfile::Codex, bounds(), command, 80, 24)
        .expect("测试 Agent 替身应可成功 spawn");
    let registered = runtime.take_events();
    assert_eq!(
        registered.len(),
        1,
        "测试开始前应只清空 NodeRegistered 事件"
    );

    let changed = runtime.apply_agent_task_update(update(
        node_id,
        AgentTaskState::Thinking,
        "UserPromptSubmit",
    ));

    assert!(changed, "首次 task update 应返回 true，表示状态写入成功");
    assert_eq!(
        runtime.agent_task_state(node_id),
        Some(AgentTaskState::Thinking),
        "Agent node 的 task state 应更新为 Thinking"
    );
    let events = runtime.take_events();
    assert_eq!(
        events,
        vec![RuntimeEvent::AgentTaskStateChanged {
            node_id,
            from: AgentTaskState::Ready,
            to: AgentTaskState::Thinking,
            source_event: "UserPromptSubmit".to_string(),
        }],
        "首次 Thinking update 应产生 AgentTaskStateChanged 事件"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::NodeRunStateChanged { .. })),
        "Agent task update 不应伪造成 PTY run state 变化"
    );

    runtime.kill_all();
}

/// 重复 Agent task state 不应重复发 AgentTaskStateChanged。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node_id=1
///   两次 update 都是 RunningTool{tool_name=Some("Bash")}
///   第一次应产生事件，第二次 from/to 相同，应被去重
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Agent node 并清空注册事件
///   2. 第一次 apply RunningTool(Bash) → 写入状态并发事件
///   3. take_events 清空第一次状态事件
///   4. 第二次 apply RunningTool(Bash) → 检测重复状态
///
/// 预期结果：
///   - 正断言：第一次 update 返回 true
///   - 正断言：第二次 update 返回 false
///   - 负断言：第二次 take_events 为空
#[test]
fn test_duplicate_agent_task_state_does_not_emit_duplicate_event() {
    let mut runtime = Runtime::new();
    let command = PtyCommand::new("/bin/sh").args(["-lc", "exit 0"]);
    let node_id = runtime
        .spawn_agent_with_command(AgentProfile::Codex, bounds(), command, 80, 24)
        .expect("测试 Agent 替身应可成功 spawn");
    let registered = runtime.take_events();
    assert_eq!(
        registered.len(),
        1,
        "测试开始前应只清空 NodeRegistered 事件"
    );
    let task_state = AgentTaskState::RunningTool {
        tool_name: Some("Bash".to_string()),
    };

    let first = runtime.apply_agent_task_update(update(node_id, task_state.clone(), "PreToolUse"));
    let first_events = runtime.take_events();
    let second = runtime.apply_agent_task_update(update(node_id, task_state, "PreToolUse"));
    let second_events = runtime.take_events();

    assert!(first, "第一次 RunningTool update 应写入状态");
    assert_eq!(
        first_events.len(),
        1,
        "第一次 RunningTool update 应产生一条状态事件"
    );
    assert!(
        !second,
        "第二次相同 RunningTool update 应返回 false，表示没有状态变化"
    );
    assert!(
        second_events.is_empty(),
        "重复状态不应重复发事件，实际: {:?}",
        second_events
    );

    runtime.kill_all();
}

/// 非 Agent node 的 task update 应被忽略，不能污染 task state map。
///
/// 数据构造（含关键数值的推导过程）：
///   Preview node_id=1，由 spawn_preview_from_path 创建
///   update.profile=Codex，to=Thinking，但目标不是 Agent
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Preview node
///   2. apply_agent_task_update(node_id=Preview)
///   3. 查询 task state 和 runtime events
///
/// 预期结果：
///   - 正断言：apply 返回 false，表示没有状态变化
///   - 正断言：agent_task_state(preview)=None
///   - 负断言：不应产生 AgentTaskStateChanged
#[test]
fn test_agent_task_update_ignores_non_agent_node() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Preview(PreviewSpec {
            path: "README.md".to_string(),
            explicit_type: None,
        }),
        bounds(),
    );

    let changed = runtime.apply_agent_task_update(update(
        node_id,
        AgentTaskState::Thinking,
        "UserPromptSubmit",
    ));
    let events = runtime.take_events();

    assert!(!changed, "非 Agent node 的 task update 应被忽略");
    assert_eq!(
        runtime.agent_task_state(node_id),
        None,
        "非 Agent node 不应写入 task state"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, RuntimeEvent::AgentTaskStateChanged { .. })),
        "非 Agent node 不应产生 AgentTaskStateChanged，实际: {:?}",
        events
    );
}

/// remove_node 应清理 Agent task state，避免关闭卡片后残留旧任务状态。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node_id=1
///   task state=WaitingApproval{tool_name=Some("Bash")}
///   remove_node(node_id) 后 document/live/task state 都应清理
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Agent node
///   2. apply WaitingApproval(Bash) 并确认状态存在
///   3. remove_node(node_id)
///   4. 查询 agent_task_state(node_id)
///
/// 预期结果：
///   - 正断言：remove_node 返回 true
///   - 正断言：agent_task_state(node_id)=None
///   - 负断言：删除后不应还能查询到 WaitingApproval
#[test]
fn test_remove_node_clears_agent_task_state() {
    let mut runtime = Runtime::new();
    let command = PtyCommand::new("/bin/sh").args(["-lc", "exit 0"]);
    let node_id = runtime
        .spawn_agent_with_command(AgentProfile::Codex, bounds(), command, 80, 24)
        .expect("测试 Agent 替身应可成功 spawn");
    let registered = runtime.take_events();
    assert_eq!(
        registered.len(),
        1,
        "测试开始前应只清空 NodeRegistered 事件"
    );
    let waiting = AgentTaskState::WaitingApproval {
        tool_name: Some("Bash".to_string()),
    };
    let changed =
        runtime.apply_agent_task_update(update(node_id, waiting.clone(), "PermissionRequest"));
    assert!(changed, "WaitingApproval update 应写入状态");
    assert_eq!(
        runtime.agent_task_state(node_id),
        Some(waiting.clone()),
        "remove 前应能查询到 WaitingApproval 状态"
    );

    let removed = runtime.remove_node(node_id);

    assert!(removed, "remove_node 应返回 true");
    assert_eq!(
        runtime.agent_task_state(node_id),
        None,
        "remove_node 后 task state 应被清理"
    );
    assert_ne!(
        runtime.agent_task_state(node_id),
        Some(waiting),
        "删除后不应残留 WaitingApproval 状态"
    );
}
