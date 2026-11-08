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

/// Grok 回合结束时 backgroundTasks 仍列出已结束任务：卡片不能停在运行中。
///
/// 数据构造：node=318。Henry 会话 01a1b583 在 2026-11-07T10:38:25Z 的 Stop
/// 之前，快照里只剩 status=completed / failed 的任务（69a53873、b1e88d6f、f6516e37），
/// 没有 running。对照快照再加一条 status=running。
///
/// 执行过程：已结束快照 observe 后 poll 到 800ms；另一判定器观察混合快照后同样 poll。
///
/// 预期结果：
///   - 正断言：只有 completed/failed 时，800ms 窗口后 Confirm，且 background_unsettled=false。
///   - 负断言：窗口差 1ms 不确认；混有 running 时到窗口结束仍不是 Confirm。
#[test]
fn grok_stop_with_only_settled_background_tasks_confirms() {
    let mut judge = judge();
    let settled = judge.observe(
        &grok_stop(json!([
            {"id": "69a53873-0ff8-4b75-be10-96171cf23726", "type": "shell", "status": "completed"},
            {"id": "b1e88d6f-a6dd-445e-8abe-f0722798616b", "type": "shell", "status": "failed"},
            {"id": "f6516e37-0bf1-4dc3-b652-08ec5a3fcbec", "type": "shell", "status": "failed"}
        ])),
        1_000,
    );
    assert!(
        matches!(settled.as_slice(), [CompletionAction::KeepThinking { .. }]),
        "已结束任务的 Stop 先进入确认窗口，实际 {settled:?}"
    );
    assert_eq!(
        quiet_until_confirm(&mut judge, 1_000),
        vec![false],
        "没有仍在跑的后台任务时，800ms 后卡片必须完成且不是未结清"
    );

    let mut mixed = CompletionJudge::new();
    mixed.observe(
        &grok_stop(json!([
            {"id": "done-1", "status": "completed"},
            {"id": "live-1", "status": "running"}
        ])),
        1_000,
    );
    assert!(
        confirm_flags(&mixed.poll(1_000 + CONFIRMATION_WINDOW_MS)).is_empty(),
        "还有 running 任务时不得因为旁边的 completed 提前完成"
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

/// Grok 回合被中断时发 StopCancelled 而不是 Stop，卡片必须离开运行态并清掉账本。
///
/// 数据构造：先一个含 running 任务的 Stop（保持 Thinking），再 StopCancelled(user_interrupt)。
///
/// 执行过程：逐个 observe 后 poll 到窗口之后。
///
/// 预期结果：
///   - 正断言：StopCancelled 被 Forward，映射为 Ended；节点不再被账本跟踪。
///   - 负断言：之后没有迟到的 Confirm。
#[test]
fn grok_stop_cancelled_ends_turn_and_clears_ledger() {
    let mut judge = judge();
    judge.observe(&grok_stop(json!([{"id": "t1", "status": "running"}])), 0);
    let cancelled = grok(json!({"hook_event_name": "StopCancelled", "reason": "user_interrupt"}));
    assert_eq!(
        norma::agent_hook::task_state_for_provider_event(&cancelled),
        Some(norma::agent_hook::AgentHookTaskState::Ended)
    );
    let actions = judge.observe(&cancelled, 100);
    assert!(
        matches!(actions.as_slice(), [CompletionAction::Forward(event)] if event.provider_event == "StopCancelled"),
        "实际 {actions:?}"
    );
    assert!(judge.tracked_nodes().is_empty());
    assert!(confirm_flags(&judge.poll(10_000)).is_empty());
}

/// Grok 回合结束时另一条队列发 `Notification(task_complete)`，常晚于 Stop 到达。
///
/// 数据构造：node=318。空 Stop 后 10ms 到达 notificationType=task_complete。
///
/// 执行过程：空 Stop(t=0) → Notification(t=10) → poll 到窗口结束。
///
/// 预期结果：
///   - 正断言：窗口结束得到一次 Confirm(unsettled=false)，卡片能完成。
///   - 负断言：Notification 不得作废确认窗口，否则卡片永久停在运行中。
#[test]
fn grok_notification_after_stop_does_not_cancel_confirmation() {
    let mut judge = judge();
    judge.observe(&grok_stop(json!([])), 0);
    judge.observe(
        &grok(json!({"hook_event_name": "Notification", "notificationType": "task_complete"})),
        10,
    );
    assert_eq!(quiet_until_confirm(&mut judge, 0), vec![false]);
}

fn grok_prompt(prompt_id: &str) -> AgentHookEvent {
    grok(json!({"hook_event_name": "UserPromptSubmit", "prompt": "hi", "promptId": prompt_id}))
}

fn grok_idle() -> AgentHookEvent {
    grok(json!({"hook_event_name": "Notification", "notificationType": "idle_prompt"}))
}

/// Grok 漏发 Stop 时，idle_prompt 兜底把开放回合收尾为 Done。
///
/// 数据构造：node=318。UserPromptSubmit(p1) 后没有任何结束报告，60s 后到达 idle_prompt。
///
/// 执行过程：prompt → idle_prompt。
///
/// 预期结果：
///   - 正断言：idle_prompt 被 Forward，且映射为 Done。
///   - 负断言：第二次 idle_prompt（回合已收尾）不再产生任何动作。
#[test]
fn grok_idle_prompt_settles_turn_without_stop() {
    let mut judge = judge();
    judge.observe(&grok_prompt("p1"), 0);
    let actions = judge.observe(&grok_idle(), 60_000);
    let [CompletionAction::Forward(event)] = actions.as_slice() else {
        panic!("开放回合的 idle_prompt 必须 Forward，实际 {actions:?}");
    };
    assert_eq!(
        norma::agent_hook::task_state_for_provider_event(event),
        Some(norma::agent_hook::AgentHookTaskState::Done)
    );
    let again = judge.observe(&grok_idle(), 120_000);
    assert!(
        again.is_empty(),
        "已收尾回合的 idle_prompt 不得再改状态，实际 {again:?}"
    );
}

/// 已确认完成、被中断或后台任务未清时，idle_prompt 不得改写状态。
///
/// 数据构造：node=318。三种情形各一个 judge：Stop 已确认 / StopCancelled / 账本含 running 任务。
///
/// 执行过程：各自走到对应状态后发 idle_prompt。
///
/// 预期结果：
///   - 正断言：三种情形都得到空动作。
///   - 负断言：不会把 Ended 改成 Done，也不会提前结束后台任务。
#[test]
fn grok_idle_prompt_ignored_outside_open_quiet_turn() {
    let mut done = judge();
    done.observe(&grok_prompt("p1"), 0);
    done.observe(&grok_stop(json!([])), 10);
    assert_eq!(quiet_until_confirm(&mut done, 10), vec![false]);
    assert!(done.observe(&grok_idle(), 60_000).is_empty());

    let mut cancelled = judge();
    cancelled.observe(&grok_prompt("p1"), 0);
    cancelled.observe(
        &grok(json!({"hook_event_name": "StopCancelled", "reason": "user_interrupt", "promptId": "p1"})),
        10,
    );
    assert!(cancelled.observe(&grok_idle(), 60_000).is_empty());

    let mut background = judge();
    background.observe(&grok_prompt("p1"), 0);
    background.observe(
        &grok_stop(json!([{"id": "m1", "type": "monitor", "status": "running"}])),
        10,
    );
    assert!(background.observe(&grok_idle(), 60_000).is_empty());
}

/// 上一回合的结束报告晚于下一回合 UserPromptSubmit 到达时按 promptId 丢弃。
///
/// 数据构造：node=318。prompt(p1) → prompt(p2) → StopCancelled(p1) / Stop(p1)，最后 Stop(p2)。
///
/// 执行过程：依次 observe，再等窗口。
///
/// 预期结果：
///   - 正断言：Stop(p2) 过窗口后 Confirm 一次。
///   - 负断言：p1 的迟到报告不产生任何动作，不会把 p2 标成 Ended 或提前 Done。
#[test]
fn grok_late_report_from_previous_prompt_is_dropped() {
    let mut judge = judge();
    judge.observe(&grok_prompt("p1"), 0);
    judge.observe(&grok_prompt("p2"), 100);
    let cancelled = judge.observe(
        &grok(json!({"hook_event_name": "StopCancelled", "reason": "user_interrupt", "promptId": "p1"})),
        110,
    );
    assert!(
        cancelled.is_empty(),
        "上一回合的 StopCancelled 必须丢弃，实际 {cancelled:?}"
    );
    let stale_stop = judge.observe(
        &grok(json!({"hook_event_name": "Stop", "reason": "end_turn", "backgroundTasks": [], "promptId": "p1"})),
        120,
    );
    assert!(
        stale_stop.is_empty(),
        "上一回合的 Stop 必须丢弃，实际 {stale_stop:?}"
    );
    assert!(confirm_flags(&judge.poll(120 + CONFIRMATION_WINDOW_MS)).is_empty());
    judge.observe(
        &grok(json!({"hook_event_name": "Stop", "reason": "end_turn", "backgroundTasks": [], "promptId": "p2"})),
        2_000,
    );
    assert_eq!(quiet_until_confirm(&mut judge, 2_000), vec![false]);
}
