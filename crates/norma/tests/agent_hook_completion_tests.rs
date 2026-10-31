#[path = "support/completion_hook.rs"]
mod support;

use norma::agent_hook::{parse_hook_event, CompletionAction, HookSource};
use serde_json::json;
use support::{
    background_bash, claude, confirm_flags, empty_stop, judge, notification, quiet_until_confirm,
    stop_with_tasks,
};

/// 单个后台 Bash：非空 Stop 不得完成；结清后的空 Stop 过窗口才完成。
///
/// 数据构造：
///   node=7
///   PostToolUse.tool_response.backgroundTaskId="bd0jz0gti"
///   第一次 Stop.background_tasks=[{id:"bd0jz0gti", status:"running"}]
///   task-notification status=completed 后，第二次 Stop.background_tasks=[]
///   确认窗口 800ms：799 不确认，800 确认
///
/// 执行过程：
///   1. 登记后台 Bash，再喂非空 Stop。
///   2. 在 800ms 处 poll。
///   3. notification 按 task-id 结清，再喂空 Stop，poll 到窗口结束。
///
/// 预期结果：
///   - 正断言：空 Stop 后 800ms 确认，且 background_unsettled=false。
///   - 负断言：非空 Stop 后 800ms 没有 Confirm；notification 本身也不立即 Confirm。
#[test]
fn single_background_bash_waits_for_empty_stop_and_window() {
    let mut judge = judge();
    let node = 7;
    judge.observe(
        &claude(node, background_bash(node, "bd0jz0gti", "toolu_bg")),
        0,
    );
    let held = judge.observe(&claude(node, stop_with_tasks(node, &["bd0jz0gti"])), 1_000);
    assert!(
        confirm_flags(&held).is_empty(),
        "后台任务仍在 Stop 快照里时，不得立刻 Confirm"
    );
    assert!(
        confirm_flags(&judge.poll(1_000 + 800)).is_empty(),
        "非空账本不能靠 800ms 窗口放行"
    );

    let woke = judge.observe(
        &claude(node, notification("bd0jz0gti", "toolu_bg", "completed")),
        2_177,
    );
    assert!(
        confirm_flags(&woke).is_empty(),
        "task-notification 只结清 id，不代替下一次空 Stop"
    );
    judge.observe(&claude(node, empty_stop()), 3_000);
    let flags = quiet_until_confirm(&mut judge, 3_000);
    assert_eq!(
        flags,
        vec![false],
        "全部结清并安静 800ms 后才确认，且不是超时降级"
    );
}

/// 两个后台任务：快照里还剩一个时不得完成。
///
/// 数据构造：
///   短任务 bjc3a4knv，长任务 bsbkuauz1
///   第一次 Stop 快照只剩 bsbkuauz1（短任务已结束）
///   最后一次 Stop.background_tasks=[]
///
/// 执行过程：登记两个 → 第一次 Stop → 结清长任务 → 空 Stop → 窗口。
///
/// 预期结果：
///   - 正断言：最后的空 Stop 在 800ms 确认。
///   - 负断言：只剩一个 id 的 Stop 不确认。
#[test]
fn first_of_two_background_tasks_does_not_release_the_turn() {
    let mut judge = judge();
    let node = 7;
    judge.observe(
        &claude(node, background_bash(node, "bsbkuauz1", "toolu_a")),
        0,
    );
    judge.observe(
        &claude(node, background_bash(node, "bjc3a4knv", "toolu_b")),
        0,
    );
    judge.observe(&claude(node, stop_with_tasks(node, &["bsbkuauz1"])), 100);
    assert!(
        confirm_flags(&judge.poll(100 + 800)).is_empty(),
        "快照仍含 bsbkuauz1 时，短任务先完成也不得 Done"
    );
    judge.observe(
        &claude(node, notification("bsbkuauz1", "toolu_a", "completed")),
        200,
    );
    judge.observe(&claude(node, empty_stop()), 300);
    assert_eq!(
        quiet_until_confirm(&mut judge, 300),
        vec![false],
        "两个 id 都离开账本后，空 Stop 的窗口结束才确认"
    );
}

/// TaskStop 用 tool_input.task_id 结清，随后空 Stop 才能进入窗口。
///
/// 数据构造：backgroundTaskId=b845c8imr；TaskStop 的 input 与 response 都是这个 id。
///
/// 执行过程：登记 → TaskStop → 空 Stop → poll 799/800。
///
/// 预期结果：
///   - 正断言：800ms 确认且标记为已结清。
///   - 负断言：TaskStop 本身不产生 Confirm。
#[test]
fn task_stop_clears_the_background_id_before_the_empty_stop() {
    let mut judge = judge();
    let node = 7;
    judge.observe(
        &claude(node, background_bash(node, "b845c8imr", "toolu_kill")),
        0,
    );
    let killed = judge.observe(
        &claude(
            node,
            json!({
                "hook_event_name": "PostToolUse",
                "tool_name": "TaskStop",
                "tool_input": {"task_id": "b845c8imr"},
                "tool_response": {"task_id": "b845c8imr", "message": "stopped"}
            }),
        ),
        50,
    );
    assert!(
        confirm_flags(&killed).is_empty(),
        "TaskStop 只删 id，不等同于回合完成"
    );
    judge.observe(&claude(node, empty_stop()), 80);
    assert_eq!(
        quiet_until_confirm(&mut judge, 80),
        vec![false],
        "显式 TaskStop 之后的空 Stop 应在 800ms 确认"
    );
}

/// Stop 先到、后台登记后到：窗口内的登记必须取消完成。
///
/// 数据构造：t=0 的空 Stop，t=100 的 PostToolUse(run_in_background)。
/// 窗口从 0 起算，100 < 800，所以 900 时仍不能确认。
///
/// 执行过程：空 Stop → 迟到的登记 → poll(900) → 新的空 Stop(1000) → poll(1800)。
///
/// 预期结果：
///   - 正断言：第二次空 Stop 在 1800 确认。
///   - 负断言：第一次窗口被登记作废，900 没有 Confirm。
#[test]
fn stop_arriving_before_background_registration_stays_open() {
    let mut judge = judge();
    let node = 7;
    judge.observe(&claude(node, empty_stop()), 0);
    judge.observe(
        &claude(node, background_bash(node, "late-task", "toolu_late")),
        100,
    );
    assert!(
        confirm_flags(&judge.poll(900)).is_empty(),
        "800ms 窗口内迟到的后台登记必须取消这次完成"
    );
    judge.observe(&claude(node, empty_stop()), 1_000);
    assert_eq!(
        quiet_until_confirm(&mut judge, 1_000),
        vec![false],
        "登记被后续空快照替换掉之后，新窗口结束才确认"
    );
}

/// 前台工具、失败工具和其他节点都不进入这个节点的账本。
///
/// 数据构造：
///   node=7 的前台 Bash（没有 run_in_background）与 PostToolUseFailure
///   node=8 的后台任务不得影响 node=7
///
/// 执行过程：node=7 前台 + 失败 + 缺字段 Stop；node=8 只登记后台。
///
/// 预期结果：
///   - 正断言：node=7 的空账本 Stop 在 800ms 确认。
///   - 负断言：node=8 在同一时刻不确认。
#[test]
fn foreground_failure_and_other_nodes_do_not_enter_the_ledger() {
    let mut judge = judge();
    judge.observe(
        &claude(
            7,
            json!({
                "hook_event_name": "PostToolUse",
                "tool_name": "Bash",
                "tool_use_id": "toolu_fg",
                "tool_input": {"command": "echo hi", "run_in_background": false}
            }),
        ),
        0,
    );
    judge.observe(
        &claude(
            7,
            json!({
                "hook_event_name": "PostToolUseFailure",
                "tool_name": "Bash",
                "tool_use_id": "toolu_fail",
                "tool_input": {"command": "sleep 9", "run_in_background": true}
            }),
        ),
        10,
    );
    judge.observe(
        &claude(8, background_bash(8, "other-node", "toolu_other")),
        20,
    );
    judge.observe(
        &claude(
            7,
            json!({"hook_event_name": "Stop", "session_id": "session-a"}),
        ),
        30,
    );
    assert_eq!(
        quiet_until_confirm(&mut judge, 30),
        vec![false],
        "缺字段且账本为空的旧 Stop 仍应在 800ms 确认"
    );
    assert!(
        confirm_flags(&judge.poll(30 + 800)).is_empty(),
        "node=8 的后台任务还在，不能被 node=7 的完成带走"
    );
}

/// 缺 background_tasks 字段不得把已登记的后台任务清掉。
///
/// 数据构造：已登记 bd0jz0gti；Stop JSON 没有 background_tasks。
///
/// 执行过程：登记 → 缺字段 Stop → poll(800) → 带空数组的 Stop → 窗口。
///
/// 预期结果：
///   - 正断言：后来的空数组 Stop 在其自身 800ms 确认。
///   - 负断言：缺字段 Stop 后 800ms 不确认。
#[test]
fn missing_background_tasks_field_is_not_an_empty_snapshot() {
    let mut judge = judge();
    let node = 7;
    judge.observe(
        &claude(node, background_bash(node, "bd0jz0gti", "toolu_bg")),
        0,
    );
    judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "Stop", "session_id": "session-a"}),
        ),
        10,
    );
    assert!(
        confirm_flags(&judge.poll(810)).is_empty(),
        "字段缺失不是空快照，已登记的后台任务必须留下"
    );
    judge.observe(&claude(node, empty_stop()), 900);
    assert_eq!(
        quiet_until_confirm(&mut judge, 900),
        vec![false],
        "只有显式空数组才替换账本并开始确认窗口"
    );
}

/// notification 里的 tool-use-id 也能删掉以 backgroundTaskId 登记的工作。
///
/// 数据构造：work id=bd0jz0gti，别名 tool_use_id=toolu_alias。
/// notification 只带 tool-use-id，不带匹配的 task-id。
///
/// 执行过程：登记 → 非空 Stop → 只含别名的 notification → 空 Stop。
///
/// 预期结果：
///   - 正断言：空 Stop 后 800ms 确认。
///   - 负断言：别名 notification 之后、空 Stop 之前不确认。
#[test]
fn task_notification_can_settle_by_tool_use_id_alias() {
    let mut judge = judge();
    let node = 7;
    judge.observe(
        &claude(node, background_bash(node, "bd0jz0gti", "toolu_alias")),
        0,
    );
    judge.observe(&claude(node, stop_with_tasks(node, &["bd0jz0gti"])), 10);
    let woke = judge.observe(
        &claude(
            node,
            json!({
                "hook_event_name": "UserPromptSubmit",
                "prompt": "<task-notification><tool-use-id>toolu_alias</tool-use-id><status>completed</status></task-notification>"
            }),
        ),
        20,
    );
    assert!(
        confirm_flags(&woke).is_empty() && confirm_flags(&judge.poll(900)).is_empty(),
        "别名结清后仍要等下一次空 Stop，不能直接 Done"
    );
    judge.observe(&claude(node, empty_stop()), 1_000);
    assert_eq!(
        quiet_until_confirm(&mut judge, 1_000),
        vec![false],
        "tool-use-id 别名删除后，空 Stop 的窗口应能确认"
    );
}

/// 子 Agent 的 SubagentStop 快照仍列出自己时，不能当成已结清。
///
/// 数据构造：
///   agentId=aaa94264eb612c225，status=async_launched
///   SubagentStop.background_tasks 仍是该 id 且 status=running
///   父 Stop 空数组之后 36ms 到来 notification
///
/// 执行过程：启动 → SubagentStop → 父空 Stop → 36ms notification → 再一次父空 Stop。
///
/// 预期结果：
///   - 正断言：第二次父空 Stop 在 800ms 确认。
///   - 负断言：SubagentStop 后以及第一次父 Stop 后 800ms 都不确认。
#[test]
fn subagent_stop_snapshot_does_not_finish_while_the_agent_is_still_listed() {
    let mut judge = judge();
    let node = 7;
    let agent_id = "aaa94264eb612c225";
    judge.observe(
        &claude(
            node,
            json!({
                "hook_event_name": "SubagentStart",
                "agent_id": agent_id,
                "agent_type": "general-purpose"
            }),
        ),
        0,
    );
    judge.observe(
        &claude(
            node,
            json!({
                "hook_event_name": "PostToolUse",
                "tool_name": "Agent",
                "tool_use_id": "toolu_agent",
                "tool_input": {"run_in_background": true},
                "tool_response": {"agentId": agent_id, "status": "async_launched"}
            }),
        ),
        0,
    );
    let sub_stop = judge.observe(
        &claude(
            node,
            json!({
                "hook_event_name": "SubagentStop",
                "agent_id": agent_id,
                "background_tasks": [{"id": agent_id, "type": "subagent", "status": "running"}]
            }),
        ),
        100,
    );
    assert!(
        matches!(sub_stop.as_slice(), [CompletionAction::Forward(_)]),
        "SubagentStop 应转发给卡片，但原始映射不是 Done"
    );
    assert!(
        confirm_flags(&judge.poll(900)).is_empty(),
        "快照仍含 running 子 Agent 时不得完成"
    );
    judge.observe(&claude(node, empty_stop()), 1_000);
    judge.observe(
        &claude(node, notification(agent_id, "toolu_agent", "completed")),
        1_036,
    );
    assert!(
        confirm_flags(&judge.poll(1_000 + 800)).is_empty(),
        "空 Stop 后 36ms 的 notification 必须作废这次窗口"
    );
    judge.observe(&claude(node, empty_stop()), 2_000);
    assert_eq!(
        quiet_until_confirm(&mut judge, 2_000),
        vec![false],
        "下一次空 Stop 安静 800ms 后才是子 Agent 真正完成"
    );
}

/// 非 Claude 的 Stop 保持立即转发，不进入账本，也不等窗口。
///
/// 数据构造：Codex Stop，node=9。时间直接跳到 30 分钟之后。
///
/// 执行过程：observe 一次，再 poll。
///
/// 预期结果：
///   - 正断言：唯一动作是 Forward，且 provider_event=Stop。
///   - 负断言：没有任何 Confirm 或 Probe。
#[test]
fn non_claude_stop_stays_immediate() {
    let mut judge = judge();
    let event = parse_hook_event(HookSource::Codex, 9, json!({"hook_event_name": "Stop"}))
        .expect("Codex Stop 应可解析");
    let actions = judge.observe(&event, 0);
    assert!(
        matches!(actions.as_slice(), [CompletionAction::Forward(forwarded)] if forwarded.provider_event == "Stop"),
        "Codex Stop 必须原样转发，实际 {actions:?}"
    );
    assert!(
        judge.poll(30 * 60 * 1000).is_empty(),
        "未接入的 profile 不应被确认窗口或超时再发一次完成"
    );
}
