use super::*;
use canvas::CanvasRect;
use norma::agent_hook::{parse_hook_event, HookSource};
use serde_json::json;

/// drain 在后台任务未结清时不得把 Stop 交给完成副作用。
///
/// 数据构造：
///   Claude 卡 1 张
///   PostToolUse.backgroundTaskId = "bd0jz0gti"
///   第一次 Stop.background_tasks 含该 id，时间 1000
///   800ms 后的空轮询仍不得完成
///   t=3000 的 task-notification + 空 Stop 打开窗口
///   t=3800 = 3000+800 才确认
///
/// 执行过程：
///   1. 用 TestPty 创建 Claude 卡，经 hook channel drain。
///   2. 非空 Stop 后立即 drain，再在 +800ms drain。
///   3. 结清并空 Stop 后，窗口到期再 drain。
///
/// 预期结果：
///   - 正断言：到期后 terminal_tasks 恰好 1 条，状态 Done，changed=true，标记为已结清。
///   - 负断言：持有期间 terminal_tasks 为空，事件流里没有 Stop，状态不是 Done。
#[test]
fn drain_keeps_background_stop_off_the_completion_path() {
    let mut runtime = Runtime::new();
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            canvas::AgentProfile::Claude,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            80,
            24,
        )
        .expect("测试应能创建 Claude 卡");
    let (sender, receiver) = crossbeam_channel::unbounded();
    let server = hook_server_for_test(receiver);
    let hook = |raw| parse_hook_event(HookSource::Claude, node, raw).expect("hook 应可解析");
    sender
        .send(hook(json!({
            "hook_event_name": "PostToolUse",
            "session_id": "session-a",
            "tool_name": "Bash",
            "tool_use_id": "toolu_bg",
            "tool_input": {"command": "sleep 2", "run_in_background": true},
            "tool_response": {"backgroundTaskId": "bd0jz0gti"}
        })))
        .expect("后台登记应进入队列");
    sender
        .send(hook(json!({
            "hook_event_name": "Stop",
            "session_id": "session-a",
            "stop_hook_active": false,
            "background_tasks": [{"id": "bd0jz0gti", "status": "running"}]
        })))
        .expect("非空 Stop 应进入队列");

    let held = server.drain_into_runtime_at(&mut runtime, None, 1_000);
    assert!(
        held.terminal_tasks.is_empty(),
        "后台任务仍在时 drain 不得产生终态任务"
    );
    assert!(
        held.events
            .iter()
            .all(|event| event.provider_event != "Stop"),
        "未确认的 Stop 不得进入会触发播报和移动端完成的事件列表"
    );
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Thinking),
        "未确认的 Stop 应让卡片保持思考"
    );
    assert_ne!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Done),
        "账本非空时不得进入 Done"
    );

    let still = server.drain_into_runtime_at(&mut runtime, None, 1_000 + 800);
    assert!(
        still.terminal_tasks.is_empty(),
        "非空账本过了 800ms 仍不得完成"
    );

    sender
        .send(hook(json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "session-a",
            "prompt": "<task-notification><task-id>bd0jz0gti</task-id><status>completed</status></task-notification>"
        })))
        .expect("结清通知应进入队列");
    sender
        .send(hook(json!({
            "hook_event_name": "Stop",
            "session_id": "session-a",
            "stop_hook_active": false,
            "background_tasks": []
        })))
        .expect("空 Stop 应进入队列");
    let opened = server.drain_into_runtime_at(&mut runtime, None, 3_000);
    assert!(
        opened.terminal_tasks.is_empty(),
        "空 Stop 当帧只打开确认窗口，不得立刻完成"
    );
    let done = server.drain_into_runtime_at(&mut runtime, None, 3_000 + 800);
    assert_eq!(
        done.terminal_tasks.len(),
        1,
        "窗口结束应恰好交出一条终态任务"
    );
    assert_eq!(
        done.terminal_tasks[0].node_id, node,
        "终态任务必须属于这张 Claude 卡"
    );
    assert!(
        !done.terminal_tasks[0].result.background_unsettled,
        "正常结清不得带上超时未结清标记"
    );
    assert!(done.changed, "延迟确认必须把 changed 设为 true");
    assert!(
        done.events
            .iter()
            .any(|event| event.provider_event == "Stop"),
        "确认之后才把原来的 Stop 交给下游"
    );
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Done),
        "窗口结束后卡片才是 Done"
    );
}

/// 空账本窗口内的 SessionEnd 先发布 Done，再发布 Ended。
///
/// 数据构造：同一 drain 里 t=34 先到空 Stop，再到 SessionEnd。34 < 800。
///
/// 执行过程：两条 hook 进同一队列，一次 drain_into_runtime_at，再在 34+800 drain。
///
/// 预期结果：
///   - 正断言：terminal_tasks 顺序是 Done 然后 Ended；事件里 Stop 在 SessionEnd 之前。
///   - 负断言：到期再 drain 不再产生终态任务。最终卡片状态是 Ended，不是停在 Done。
#[test]
fn drain_session_end_in_the_quiet_window_publishes_done_before_ended() {
    let mut runtime = Runtime::new();
    runtime.set_pty_factory(Box::new(|_, _, _, tx| {
        Ok(Box::new(terminal::TestPtyBackend::echo(tx)))
    }));
    let node = runtime
        .spawn_agent(
            canvas::AgentProfile::Claude,
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
            80,
            24,
        )
        .expect("测试应能创建 Claude 卡");
    let (sender, receiver) = crossbeam_channel::unbounded();
    let server = hook_server_for_test(receiver);
    let hook = |raw| parse_hook_event(HookSource::Claude, node, raw).expect("hook 应可解析");
    sender
        .send(hook(json!({
            "hook_event_name": "Stop",
            "session_id": "session-a",
            "stop_hook_active": false,
            "background_tasks": []
        })))
        .expect("空 Stop 应进入队列");
    sender
        .send(hook(json!({
            "hook_event_name": "SessionEnd",
            "session_id": "session-a"
        })))
        .expect("SessionEnd 应进入队列");

    let ended = server.drain_into_runtime_at(&mut runtime, None, 34);
    assert_eq!(
        ended.terminal_tasks.len(),
        2,
        "34ms 的 SessionEnd 必须先交出 Done，再交出 Ended"
    );
    assert_eq!(
        ended.terminal_tasks[0].settled_state,
        AgentTaskState::Done,
        "第一条终态必须是发布当时的 Done，不能改成 drain 结束时的 Ended"
    );
    assert!(
        !ended.terminal_tasks[0].result.background_unsettled,
        "空账本窗口内的 SessionEnd 是正常完成，不带未结清标记"
    );
    assert_eq!(
        ended.terminal_tasks[1].settled_state,
        AgentTaskState::Ended,
        "SessionEnd 自己的终态是 Ended"
    );
    let stop_at = ended
        .events
        .iter()
        .position(|event| event.provider_event == "Stop");
    let session_end_at = ended
        .events
        .iter()
        .position(|event| event.provider_event == "SessionEnd");
    assert!(
        stop_at.is_some_and(|stop| session_end_at.is_some_and(|end| stop < end)),
        "Stop 必须先于 SessionEnd 进入下游，实际事件 {:?}",
        ended
            .events
            .iter()
            .map(|event| event.provider_event.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        runtime.agent_task_state(node),
        Some(AgentTaskState::Ended),
        "同一 drain 结束后卡片状态是 Ended"
    );
    let later = server.drain_into_runtime_at(&mut runtime, None, 34 + 800);
    assert!(
        later.terminal_tasks.is_empty(),
        "成功已经在 SessionEnd 当帧发布，800ms 后不得再交一条 Done"
    );
}
