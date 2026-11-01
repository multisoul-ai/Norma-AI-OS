use norma::agent_hook::{
    parse_hook_event, AgentHookEvent, CompletionAction, CompletionJudge, HookSource,
    CONFIRMATION_WINDOW_MS,
};
use serde_json::{json, Value};

fn judge() -> CompletionJudge {
    CompletionJudge::new()
}

fn confirm_flags(actions: &[CompletionAction]) -> Vec<bool> {
    actions
        .iter()
        .filter_map(|action| match action {
            CompletionAction::Confirm {
                background_unsettled,
                ..
            } => Some(*background_unsettled),
            _ => None,
        })
        .collect()
}

/// 窗口差 1ms 不确认，到点才确认。
fn quiet_until_confirm(judge: &mut CompletionJudge, started_ms: u64) -> Vec<bool> {
    let early = confirm_flags(&judge.poll(started_ms + CONFIRMATION_WINDOW_MS - 1));
    assert!(early.is_empty(), "确认窗口差 1ms 不得 Done，实际 {early:?}");
    confirm_flags(&judge.poll(started_ms + CONFIRMATION_WINDOW_MS))
}

const NODE: u64 = 318;

fn grok(raw: Value) -> AgentHookEvent {
    parse_hook_event(HookSource::Grok, NODE, raw).expect("Grok hook JSON 应可解析")
}

/// Grok 的 Stop 只发 camelCase `backgroundTasks`（~/.grok/docs/user-guide/10-hooks.md）。
fn grok_stop(tasks: Value) -> AgentHookEvent {
    grok(json!({"hook_event_name": "Stop", "reason": "end_turn", "backgroundTasks": tasks}))
}

/// Grok 回合结束时仍有后台任务：保持运行；任务清空后的 Stop 过确认窗口才完成。
///
/// 数据构造：node=318。第一次 Stop 含一个 running monitor，第二次 backgroundTasks=[]。
///
/// 执行过程：非空 Stop → poll(30 分钟内) → 空 Stop → 窗口。
///
/// 预期结果：
///   - 正断言：非空 Stop 只得到 KeepThinking；空 Stop 800ms 后 Confirm(unsettled=false)。
///   - 负断言：非空 Stop 不 Forward、不 Confirm；节点受账本跟踪，进程退出可被清理。
#[test]
fn grok_stop_with_background_tasks_keeps_thinking() {
    let mut judge = judge();
    let held = judge.observe(
        &grok_stop(json!([{"id": "m1", "type": "monitor", "status": "running"}])),
        0,
    );
    assert!(
        matches!(held.as_slice(), [CompletionAction::KeepThinking { .. }]),
        "后台任务还在跑时 Grok 卡片必须保持运行，实际 {held:?}"
    );
    assert!(
        judge.tracked_nodes().contains(&NODE),
        "Grok 节点必须进入账本，进程退出时才能 abandon"
    );
    assert!(confirm_flags(&judge.poll(60_000)).is_empty());
    judge.observe(&grok_stop(json!([])), 100_000);
    assert_eq!(
        quiet_until_confirm(&mut judge, 100_000),
        vec![false],
        "任务清空后的 Stop 过窗口才完成"
    );
}

/// Grok 子会话的 SessionEnd / UserPromptSubmit / Stop 不属于父卡片，必须丢弃。
///
/// 数据构造：三个事件都带 subagentType="explore"；PreToolUse 也带 subagentType。
///
/// 执行过程：逐个 observe。
///
/// 预期结果：
///   - 正断言：子会话里的 PreToolUse 仍转发（卡片在干活）。
///   - 负断言：生命周期事件没有任何动作，父卡片不会变成 Ended / Done。
#[test]
fn grok_subagent_lifecycle_events_are_dropped() {
    let mut judge = judge();
    for name in ["SessionStart", "UserPromptSubmit", "Stop", "SessionEnd"] {
        let actions = judge.observe(
            &grok(json!({"hook_event_name": name, "subagentType": "explore"})),
            0,
        );
        assert!(
            actions.is_empty(),
            "子会话 {name} 不得影响父卡片，实际 {actions:?}"
        );
    }
    let tool = judge.observe(
        &grok(json!({"hook_event_name": "PreToolUse", "subagentType": "explore", "tool_name": "Read"})),
        0,
    );
    assert!(
        matches!(tool.as_slice(), [CompletionAction::Forward(event)] if event.provider_event == "PreToolUse"),
        "子会话的工具活动仍应显示运行，实际 {tool:?}"
    );
}

/// 会话关闭时 Grok 额外发的 Stop（reason=shutdown）不是回合完成。
///
/// 数据构造：Stop.reason="shutdown"，backgroundTasks=[]。
///
/// 执行过程：observe 后 poll 到窗口结束。
///
/// 预期结果：
///   - 负断言：没有任何动作，也没有窗口确认。
#[test]
fn grok_session_close_stop_is_not_completion() {
    let mut judge = judge();
    let actions = judge.observe(
        &grok(json!({"hook_event_name": "Stop", "reason": "shutdown", "backgroundTasks": []})),
        0,
    );
    assert!(actions.is_empty(), "关闭 Stop 不得完成，实际 {actions:?}");
    assert!(confirm_flags(&judge.poll(10_000)).is_empty());
}
