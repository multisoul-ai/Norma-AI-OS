#[path = "support/completion_hook.rs"]
mod support;

use norma::agent_hook::CompletionAction;
use norma::agent_hook::BACKGROUND_WAIT_LIMIT_MS;
use serde_json::json;
use support::{
    background_bash, claude, confirm_flags, empty_stop, judge, notification, quiet_until_confirm,
    stop_with_tasks,
};

/// 空 Stop 之后迟到的后台登记，没有下一次 Stop 时仍在 30 分钟降级完成。
///
/// 数据构造：
///   BACKGROUND_WAIT_LIMIT_MS = 30 * 60 * 1000
///   t=0 空 Stop 打开窗口并清掉超时时钟
///   t=100 PostToolUse(run_in_background)，100 < 800，窗口作废
///   边界：100+LIMIT-1 不确认，100+LIMIT 确认
///   900 = 100+800，仍远早于 30 分钟
///
/// 执行过程：空 Stop → 迟到登记 → running notification → 在 900、LIMIT 边界和 100+LIMIT 边界 poll → 新的空 Stop。
///
/// 预期结果：
///   - 正断言：t=100+LIMIT 的 Confirm.background_unsettled=true；之后的新空 Stop 在 800ms 确认且 unsettled=false。
///   - 负断言：900 与 100+LIMIT-1 都不确认；确认只发生一次。running notification 不结清。
#[test]
fn late_background_registration_still_times_out_without_another_stop() {
    let mut judge = judge();
    let node = 7;
    judge.observe(&claude(node, empty_stop()), 0);
    judge.observe(
        &claude(node, background_bash(node, "late-task", "toolu_late")),
        100,
    );
    judge.observe(
        &claude(node, notification("late-task", "toolu_late", "running")),
        200,
    );
    assert!(
        confirm_flags(&judge.poll(900)).is_empty(),
        "迟到登记后 800ms 窗口必须作废，900 不得确认"
    );
    assert!(
        confirm_flags(&judge.poll(100 + BACKGROUND_WAIT_LIMIT_MS - 1)).is_empty(),
        "迟到登记重新武装的 30 分钟还差 1ms 时不得降级"
    );
    assert_eq!(
        confirm_flags(&judge.poll(100 + BACKGROUND_WAIT_LIMIT_MS)),
        vec![true],
        "没有下一次 Stop 时，应从迟到登记起满 30 分钟带未结清标记确认"
    );
    assert!(
        confirm_flags(&judge.poll(100 + BACKGROUND_WAIT_LIMIT_MS + 800)).is_empty(),
        "超时确认只发生一次"
    );
    let restarted = 100 + BACKGROUND_WAIT_LIMIT_MS;
    judge.observe(&claude(node, empty_stop()), restarted);
    assert_eq!(
        quiet_until_confirm(&mut judge, restarted),
        vec![false],
        "超时降级清空账本后，新的空 Stop 按 800ms 正常确认且不带未结清标记"
    );
}

/// 空 Stop 之后迟到的 SubagentStart 与迟到的 PostToolUse 同一类。
///
/// 数据构造：t=0 空 Stop；t=100 SubagentStart.agent_id=aaa94264eb612c225。
/// 30 分钟从 100 起算，不是从 0 起算。
///
/// 执行过程：空 Stop → SubagentStart → poll(LIMIT) → poll(100+LIMIT)。
///
/// 预期结果：
///   - 正断言：100+LIMIT 确认且 unsettled=true。
///   - 负断言：若时钟仍停在被清掉的 0，t=LIMIT 就会确认；这里不得确认。
#[test]
fn late_subagent_start_rearms_the_timeout_without_another_stop() {
    let mut judge = judge();
    let node = 7;
    judge.observe(&claude(node, empty_stop()), 0);
    judge.observe(
        &claude(
            node,
            json!({
                "hook_event_name": "SubagentStart",
                "session_id": "session-a",
                "agent_id": "aaa94264eb612c225"
            }),
        ),
        100,
    );
    assert!(
        confirm_flags(&judge.poll(BACKGROUND_WAIT_LIMIT_MS)).is_empty(),
        "时钟必须从迟到的 SubagentStart 起算，t=LIMIT 不得沿用已被空 Stop 清掉的起点"
    );
    assert_eq!(
        confirm_flags(&judge.poll(100 + BACKGROUND_WAIT_LIMIT_MS)),
        vec![true],
        "迟到的子 Agent 登记后，没有下一次 Stop 也应在 30 分钟降级确认"
    );
}

/// 账本非空时的前台活动不得解除已经开始的 30 分钟超时。
///
/// 数据构造：t=0 的非空 Stop 开始时钟；t=100 的 PreToolUse 不是结清。
/// 若活动把 turn_ended 清掉，t=LIMIT 不会确认。
///
/// 执行过程：非空 Stop → PreToolUse → poll(LIMIT-1) → poll(LIMIT)。
///
/// 预期结果：
///   - 正断言：t=LIMIT 确认且 unsettled=true。
///   - 负断言：t=LIMIT-1 不确认。
#[test]
fn foreground_activity_does_not_disarm_an_unsettled_timeout() {
    let mut judge = judge();
    let node = 7;
    judge.observe(&claude(node, stop_with_tasks(node, &["bd0jz0gti"])), 0);
    judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"}),
        ),
        100,
    );
    assert!(
        confirm_flags(&judge.poll(BACKGROUND_WAIT_LIMIT_MS - 1)).is_empty(),
        "前台活动不得把 30 分钟提前"
    );
    assert_eq!(
        confirm_flags(&judge.poll(BACKGROUND_WAIT_LIMIT_MS)),
        vec![true],
        "非空账本上的前台活动必须留下回合已结束，30 分钟后仍降级确认"
    );
}

/// 账本非空时 SessionEnd 只结束会话，不补成功。
///
/// 数据构造：已登记且 Stop 快照仍含 bd0jz0gti；随后 SessionEnd。
///
/// 执行过程：登记 → 非空 Stop → SessionEnd → poll(20+LIMIT)。
///
/// 预期结果：
///   - 正断言：SessionEnd 只有 Forward。
///   - 负断言：动作和之后的 poll 都没有 Confirm。
#[test]
fn session_end_with_open_work_does_not_confirm() {
    let mut judge = judge();
    let node = 7;
    judge.observe(
        &claude(node, background_bash(node, "bd0jz0gti", "toolu_bg")),
        0,
    );
    judge.observe(&claude(node, stop_with_tasks(node, &["bd0jz0gti"])), 10);
    let ended = judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "SessionEnd", "session_id": "session-a"}),
        ),
        20,
    );
    assert!(
        matches!(
            ended.as_slice(),
            [CompletionAction::Forward(event)] if event.provider_event == "SessionEnd"
        ),
        "账本非空时 SessionEnd 不得补发成功，实际 {ended:?}"
    );
    assert!(
        confirm_flags(&ended).is_empty(),
        "非空账本的 SessionEnd 动作里不得有 Confirm"
    );
    assert!(
        confirm_flags(&judge.poll(20 + BACKGROUND_WAIT_LIMIT_MS)).is_empty(),
        "SessionEnd 已清账本，不得再按 30 分钟降级"
    );
}

/// 确认窗口内的 StopFailure 不把回合记成成功。
///
/// 数据构造：t=0 空 Stop；t=34 StopFailure。34 < 800。
///
/// 执行过程：空 Stop → StopFailure → poll(800) → poll(34+800)。
///
/// 预期结果：
///   - 正断言：动作只有 Forward(StopFailure)。
///   - 负断言：动作和两个 poll 都没有 Confirm。
#[test]
fn stop_failure_during_a_quiet_window_does_not_confirm() {
    let mut judge = judge();
    let node = 7;
    judge.observe(&claude(node, empty_stop()), 0);
    let failed = judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "StopFailure", "session_id": "session-a"}),
        ),
        34,
    );
    assert!(
        matches!(
            failed.as_slice(),
            [CompletionAction::Forward(event)] if event.provider_event == "StopFailure"
        ),
        "StopFailure 必须转发且不得先确认成功，实际 {failed:?}"
    );
    assert!(
        confirm_flags(&failed).is_empty(),
        "StopFailure 不是成功完成"
    );
    assert!(
        confirm_flags(&judge.poll(800)).is_empty(),
        "StopFailure 必须取消待定窗口"
    );
    assert!(
        confirm_flags(&judge.poll(34 + 800)).is_empty(),
        "失败清空账本后不得补确认"
    );
}
