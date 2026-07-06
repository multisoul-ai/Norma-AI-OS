use realtime::{
    RealtimeSessionId, RealtimeSessionState, RealtimeToolResult, RealtimeToolResultStatus,
};

fn started_state() -> RealtimeSessionState {
    let mut state = RealtimeSessionState::default();
    state
        .start_connecting(RealtimeSessionId::new("rt_callback"))
        .expect("测试会话应能从 Off 进入 Connecting");
    state.mark_started();
    state
}

fn started_state_with_limit(limit: usize) -> RealtimeSessionState {
    let mut state = RealtimeSessionState::with_callback_queue_limit(limit);
    state
        .start_connecting(RealtimeSessionId::new("rt_callback"))
        .expect("测试会话应能从 Off 进入 Connecting");
    state.mark_started();
    state
}

fn callback(
    tool_call_id: &str,
    status: RealtimeToolResultStatus,
    task_id: &str,
    summary: &str,
) -> RealtimeToolResult {
    RealtimeToolResult::task_result(
        RealtimeSessionId::new("rt_callback"),
        tool_call_id,
        status,
        task_id,
        summary,
    )
}

/// CB-1：idle realtime session 收到 callback 时应立即注入，而不是排队。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id    = "rt_callback"
///   user_speaking = false
///   assistant_speaking = false
///   callback = succeeded(task_7, "已创建 Codex 卡片")
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 started realtime session，保持 user/assistant 都不在说话
///   2. 调用 queue_or_inject_tool_result(callback)
///   3. 读取 injected / dropped 两个结果集合
///
/// 预期结果：
///   - 正断言：callback 直接出现在 injected 中
///   - 正断言：dropped 为空
///   - 负断言：idle callback 不应残留在 pending queue
#[test]
fn test_idle_callback_injects_immediately() {
    let mut state = started_state();
    let callback = callback(
        "call_idle",
        RealtimeToolResultStatus::Succeeded,
        "task_7",
        "已创建 Codex 卡片",
    );

    let update = state.queue_or_inject_tool_result(callback.clone());

    assert_eq!(
        update.injected,
        vec![callback],
        "idle callback 应立即返回到 injected 列表，而不是等待后续 flush"
    );
    assert!(
        update.dropped.is_empty(),
        "idle callback 不应触发 queue drop"
    );
    assert_eq!(
        state.pending_tool_result_count(),
        0,
        "idle callback 注入后 pending queue 应保持为空"
    );
}

/// CB-2：user 正在说话时 callback 必须先排队，直到 transcript final 再 flush。
///
/// 数据构造（含关键数值的推导过程）：
///   user_speaking = true（模拟前台已发送 input_audio_chunk）
///   assistant_speaking = false
///   callback = failed(task_8, "没有找到目标卡片")
///   pending count 期望从 0 -> 1 -> 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. started realtime session 调用 mark_user_speaking(true)
///   2. 调用 queue_or_inject_tool_result(callback)
///   3. 确认 injected 为空、pending count = 1
///   4. 调用 handle_transcript_final()
///   5. 检查 flush 结果
///
/// 预期结果：
///   - 正断言：用户说话期间 injected 为空，callback 留在 pending queue
///   - 正断言：transcript final 返回同一个 callback
///   - 负断言：flush 后 pending queue 不应继续残留旧 callback
#[test]
fn test_user_speaking_queues_callback_until_transcript_final_flushes() {
    let mut state = started_state();
    state.mark_user_speaking(true);
    let callback = callback(
        "call_user_busy",
        RealtimeToolResultStatus::Failed,
        "task_8",
        "没有找到目标卡片",
    );

    let queued = state.queue_or_inject_tool_result(callback.clone());
    let flushed = state.handle_transcript_final();

    assert!(
        queued.injected.is_empty(),
        "user speaking 时 callback 不应立刻 injected"
    );
    assert!(
        queued.dropped.is_empty(),
        "单条 queued callback 不应触发 drop"
    );
    assert_eq!(
        state.pending_tool_result_count(),
        0,
        "transcript final flush 完成后 pending queue 应清空"
    );
    assert_eq!(
        flushed.injected,
        vec![callback],
        "transcript final 应按原顺序 flush 已排队的 callback"
    );
}

/// CB-3：assistant 正在播报时 callback 必须排队，直到 assistant audio end 再 flush。
///
/// 数据构造（含关键数值的推导过程）：
///   user_speaking = false
///   assistant_speaking = true（模拟 assistant 正在播报）
///   callback = cancelled(task_9, "已停止后续操作")
///
/// 执行过程（逐步说明系统如何处理）：
///   1. started realtime session 调用 mark_assistant_speaking(true)
///   2. 调用 queue_or_inject_tool_result(callback)
///   3. 确认 callback 未立即 injected
///   4. 调用 handle_assistant_audio_end()
///   5. 读取 flush 结果
///
/// 预期结果：
///   - 正断言：assistant speaking 时 callback 先进入 pending queue
///   - 正断言：assistant audio end 会 flush 同一个 callback
///   - 负断言：audio end 后 pending queue 不应残留旧 callback
#[test]
fn test_assistant_speaking_queues_callback_until_audio_end_flushes() {
    let mut state = started_state();
    state.mark_assistant_speaking(true);
    let callback = callback(
        "call_assistant_busy",
        RealtimeToolResultStatus::Cancelled,
        "task_9",
        "已停止后续操作",
    );

    let queued = state.queue_or_inject_tool_result(callback.clone());
    let flushed = state.handle_assistant_audio_end();

    assert!(
        queued.injected.is_empty(),
        "assistant speaking 时 callback 不应抢占当前播报而立刻 injected"
    );
    assert_eq!(
        flushed.injected,
        vec![callback],
        "assistant audio end 应 flush 等待中的 callback"
    );
    assert_eq!(
        state.pending_tool_result_count(),
        0,
        "assistant audio end flush 后 pending queue 应清空"
    );
}

/// CB-4：callback queue 超出限制时应丢弃最旧项，并把剩余项按完成顺序 flush。
///
/// 数据构造（含关键数值的推导过程）：
///   queue_limit = 2（故意压到最小非平凡容量）
///   callbacks   = [task_1, task_2, task_3] 共 3 条
///   speaking    = user_speaking = true，因此 3 条都会先入队
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 queue_limit=2 创建 started realtime session，并标记 user_speaking(true)
///   2. 依次 queue 三条 callback
///   3. 第三次入队后读取 dropped
///   4. 调用 handle_transcript_final() flush
///
/// 预期结果：
///   - 正断言：第三次入队返回 dropped=[task_1]
///   - 正断言：flush 顺序保留 task_2 -> task_3
///   - 负断言：queue overflow 不应 panic，也不应把 task_1 留到 flush 中
#[test]
fn test_callback_queue_limit_drops_oldest_without_panicking() {
    let mut state = started_state_with_limit(2);
    state.mark_user_speaking(true);

    let first = callback(
        "call_1",
        RealtimeToolResultStatus::Succeeded,
        "task_1",
        "first",
    );
    let second = callback(
        "call_2",
        RealtimeToolResultStatus::Succeeded,
        "task_2",
        "second",
    );
    let third = callback(
        "call_3",
        RealtimeToolResultStatus::Succeeded,
        "task_3",
        "third",
    );

    let first_update = state.queue_or_inject_tool_result(first.clone());
    let second_update = state.queue_or_inject_tool_result(second.clone());
    let third_update = state.queue_or_inject_tool_result(third.clone());
    let flushed = state.handle_transcript_final();

    assert!(
        first_update.dropped.is_empty(),
        "第一条 callback 入队时不应被丢弃"
    );
    assert!(
        second_update.dropped.is_empty(),
        "第二条 callback 入队时不应被丢弃"
    );
    assert_eq!(
        third_update.dropped,
        vec![first],
        "第三条 callback 超过容量后应丢弃最旧的 task_1"
    );
    assert_eq!(
        flushed.injected,
        vec![second, third],
        "flush 应只保留最近两条 callback，且顺序必须是 task_2 -> task_3"
    );
}
