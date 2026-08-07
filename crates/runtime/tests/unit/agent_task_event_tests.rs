use super::*;
use crate::{AgentTaskState, AgentTaskUpdate, CardAttentionKind, CardAttentionState, RuntimeEvent};
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
        task_title: None,
        session_id: None,
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
            additional_paths: Vec::new(),
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

/// Agent Done 应创建未查看的通用卡片提醒，并允许用户查看后关闭提醒。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node_id=1
///   task update: Done，表示 hook 已通知当前任务完成
///   attention kind=AgentTaskDone，viewed=false 表示用户尚未查看完成结果
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Codex Agent node 并清空注册事件
///   2. apply_agent_task_update(Done) → 写入 task state，并创建未查看 attention
///   3. mark_card_viewed(node_id) → 用户显式查看该卡片
///   4. 再次 mark_card_viewed(node_id) → 已查看状态不应重复变化
///
/// 预期结果：
///   - 正断言：Done 后 card_attention 为 AgentTaskDone 且 viewed=false
///   - 正断言：首次 mark 返回 true，并把 viewed 改为 true
///   - 负断言：重复 mark 返回 false，不能伪造第二次状态变化
#[test]
fn test_agent_done_attention_can_be_marked_viewed_once() {
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

    let changed = runtime.apply_agent_task_update(update(node_id, AgentTaskState::Done, "Stop"));
    let attention_after_done = runtime.card_attention(node_id);
    let first_mark = runtime.mark_card_viewed(node_id);
    let attention_after_view = runtime.card_attention(node_id);
    let second_mark = runtime.mark_card_viewed(node_id);

    assert!(changed, "Done update 应改变 Agent task state");
    assert_eq!(
        attention_after_done,
        Some(CardAttentionState {
            kind: CardAttentionKind::AgentTaskDone,
            viewed: false,
        }),
        "Done 应创建未查看 AgentTaskDone attention"
    );
    assert!(first_mark, "首次查看未读 Done attention 应返回 true");
    assert_eq!(
        attention_after_view,
        Some(CardAttentionState {
            kind: CardAttentionKind::AgentTaskDone,
            viewed: true,
        }),
        "mark_card_viewed 后 attention 应保留 kind 并标记 viewed=true"
    );
    assert!(
        !second_mark,
        "重复查看已 viewed 的 attention 不应返回 true，否则 UI 会误以为状态又变化"
    );

    runtime.kill_all();
}

/// 新任务状态应清除旧完成提醒，避免上一轮 Done 的已查看状态污染下一轮任务。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent node_id=1
///   第一轮 task update: Done → AgentTaskDone attention
///   第二轮 task update: Thinking → 新任务开始，旧完成提醒不再成立
///   第三轮 task update: Done → 新完成结果应重新变成未查看
///
/// 执行过程（逐步说明系统如何处理）：
///   1. apply Done 并 mark_card_viewed → attention viewed=true
///   2. apply Thinking → 清除旧 attention
///   3. apply Done → 创建新的未查看 attention
///
/// 预期结果：
///   - 正断言：Thinking 后 card_attention=None
///   - 正断言：新 Done 后 viewed=false
///   - 负断言：新 Done 不应继承上一轮 viewed=true
#[test]
fn test_new_agent_task_cycle_resets_done_attention() {
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
    assert!(
        runtime.apply_agent_task_update(update(node_id, AgentTaskState::Done, "Stop")),
        "第一轮 Done 应写入状态"
    );
    assert!(
        runtime.mark_card_viewed(node_id),
        "第一轮 Done attention 应可标记为 viewed"
    );

    let thinking = runtime.apply_agent_task_update(update(
        node_id,
        AgentTaskState::Thinking,
        "UserPromptSubmit",
    ));
    let attention_after_thinking = runtime.card_attention(node_id);
    let done_again = runtime.apply_agent_task_update(update(node_id, AgentTaskState::Done, "Stop"));
    let attention_after_second_done = runtime.card_attention(node_id);

    assert!(thinking, "Thinking 应开始新任务周期并改变状态");
    assert_eq!(
        attention_after_thinking, None,
        "新任务开始后旧 Done attention 应被清除"
    );
    assert!(done_again, "第二轮 Done 应再次改变状态");
    assert_eq!(
        attention_after_second_done,
        Some(CardAttentionState {
            kind: CardAttentionKind::AgentTaskDone,
            viewed: false,
        }),
        "第二轮 Done 应创建新的未查看 attention"
    );
    assert_ne!(
        attention_after_second_done,
        Some(CardAttentionState {
            kind: CardAttentionKind::AgentTaskDone,
            viewed: true,
        }),
        "第二轮 Done 不应继承上一轮 viewed=true"
    );

    runtime.kill_all();
}

/// 没有未查看 attention 的节点调用 mark_card_viewed 应被安全忽略。
///
/// 数据构造（含关键数值的推导过程）：
///   Preview node_id=1，不是 Agent 且没有 attention
///   missing node_id=999，document 中不存在
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Preview node
///   2. mark_card_viewed(preview)
///   3. mark_card_viewed(999)
///
/// 预期结果：
///   - 正断言：两个 mark 都返回 false
///   - 正断言：Preview attention 仍为 None
///   - 负断言：不应凭空创建 AgentTaskDone attention
#[test]
fn test_mark_card_viewed_ignores_nodes_without_attention() {
    let mut runtime = Runtime::new();
    let preview = runtime.document.add_node(
        CanvasNodeKind::Preview(PreviewSpec {
            path: "README.md".to_string(),
            additional_paths: Vec::new(),
            explicit_type: None,
        }),
        bounds(),
    );

    let preview_marked = runtime.mark_card_viewed(preview);
    let missing_marked = runtime.mark_card_viewed(999);

    assert!(
        !preview_marked,
        "没有 attention 的 Preview 调用 mark_card_viewed 应返回 false"
    );
    assert!(
        !missing_marked,
        "不存在的 node_id 调用 mark_card_viewed 应返回 false"
    );
    assert_eq!(
        runtime.card_attention(preview),
        None,
        "Preview 不应因为 mark_card_viewed 被创建 attention"
    );
    assert_ne!(
        runtime.card_attention(preview),
        Some(CardAttentionState {
            kind: CardAttentionKind::AgentTaskDone,
            viewed: false,
        }),
        "非 Agent Preview 不应凭空出现 AgentTaskDone attention"
    );
}
