#[path = "support/completion_hook.rs"]
mod support;

use norma::agent_hook::{CompletionAction, BACKGROUND_WAIT_LIMIT_MS, CONFIRMATION_WINDOW_MS};
use serde_json::json;
use support::{
    background_bash, claude, confirm_flags, empty_stop, judge, notification, quiet_until_confirm,
    stop_with_tasks,
};

fn has_probe(actions: &[CompletionAction]) -> bool {
    actions
        .iter()
        .any(|action| matches!(action, CompletionAction::ProbeProcesses { .. }))
}

/// 新的用户 prompt 不清账本，但会取消尚未结束的确认窗口。
///
/// 数据构造：已登记的后台 id 仍在；普通 prompt 不含 task-notification。
/// 空 Stop 位于 t=0，prompt 位于 t=100，100 < 800。
///
/// 执行过程：登记 → 非空 Stop → 普通 prompt → 缺字段 Stop → poll。
///
/// 预期结果：
///   - 正断言：缺字段 Stop 后账本仍非空，800ms 不确认。
///   - 负断言：普通 prompt 没有 Confirm。
#[test]
fn a_new_prompt_does_not_clear_unsettled_background_work() {
    let mut judge = judge();
    let node = 7;
    judge.observe(
        &claude(node, background_bash(node, "bd0jz0gti", "toolu_bg")),
        0,
    );
    judge.observe(&claude(node, stop_with_tasks(node, &["bd0jz0gti"])), 10);
    let prompt = judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "UserPromptSubmit", "prompt": "继续看后台结果"}),
        ),
        20,
    );
    assert!(
        confirm_flags(&prompt).is_empty(),
        "普通 UserPromptSubmit 不是完成信号"
    );
    judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "Stop", "session_id": "session-a"}),
        ),
        30,
    );
    assert!(
        confirm_flags(&judge.poll(30 + CONFIRMATION_WINDOW_MS)).is_empty(),
        "新 prompt 没有结清 id，缺字段 Stop 不能把账本当成空的"
    );
}

/// 空账本确认窗口内的 SessionEnd 先确认成功，再转发会话结束。
///
/// 数据构造：t=0 空 Stop 已打开窗口；t=34 SessionEnd。34 < 800。
/// 取证里 `claude -p` 的 SessionEnd 比最终 Stop 晚约 34ms。
///
/// 执行过程：空 Stop → SessionEnd → poll(800) → 再 poll(34+800)。
///
/// 预期结果：
///   - 正断言：动作顺序是 Confirm(unsettled=false, Stop) 然后 Forward(SessionEnd)。
///   - 负断言：两个 poll 都不再 Confirm。
#[test]
fn session_end_confirms_a_quiet_empty_window_before_forwarding() {
    let mut judge = judge();
    let node = 7;
    judge.observe(&claude(node, empty_stop()), 0);
    let ended = judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "SessionEnd", "session_id": "session-a"}),
        ),
        34,
    );
    assert!(
        matches!(
            ended.as_slice(),
            [
                CompletionAction::Confirm {
                    background_unsettled: false,
                    event,
                },
                CompletionAction::Forward(forwarded),
            ] if event.provider_event == "Stop" && forwarded.provider_event == "SessionEnd"
        ),
        "34ms < 800ms，空窗口内的 SessionEnd 必须先确认成功再转发结束，实际 {ended:?}"
    );
    assert!(
        confirm_flags(&judge.poll(CONFIRMATION_WINDOW_MS)).is_empty(),
        "成功已在 SessionEnd 当帧确认，800ms poll 不得再确认一次"
    );
    assert!(
        confirm_flags(&judge.poll(34 + CONFIRMATION_WINDOW_MS)).is_empty(),
        "账本已清空，SessionEnd 之后也不再确认"
    );
}

/// 确认之后的新活动只转发，不再次确认。
///
/// 数据构造：无后台的空 Stop，窗口在 800ms 确认；随后 UserPromptSubmit。
///
/// 执行过程：空 Stop → poll(800) → prompt → poll(800+800)。
///
/// 预期结果：
///   - 正断言：第一次 poll 确认；prompt 是 Forward。
///   - 负断言：prompt 之后的 poll 没有第二个 Confirm。
#[test]
fn activity_after_done_is_forwarded_without_another_confirm() {
    let mut judge = judge();
    let node = 7;
    judge.observe(&claude(node, empty_stop()), 0);
    assert_eq!(
        quiet_until_confirm(&mut judge, 0),
        vec![false],
        "没有后台工作时，空 Stop 应在 800ms 确认"
    );
    let again = judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "UserPromptSubmit", "prompt": "下一步"}),
        ),
        900,
    );
    assert!(
        matches!(again.as_slice(), [CompletionAction::Forward(event)] if event.provider_event == "UserPromptSubmit"),
        "Done 之后的活动应回到转发路径，让卡片离开 Done"
    );
    assert!(
        confirm_flags(&judge.poll(900 + CONFIRMATION_WINDOW_MS)).is_empty(),
        "回退到思考后，没有新的空 Stop 不得再次完成"
    );
}

/// stop_hook_active 的 Stop 照常进入确认窗口；被阻止后续跑的活动作废窗口。
///
/// 数据构造：Claude 与 Grok 文档都说明，被 stop hook 阻止后，续跑轮次和最终那次 Stop
///   的 stop_hook_active 都是 true。旧实现对它永久 Hold，卡片会一直显示运行。
///
/// 执行过程：hook Stop(t=0) → PreToolUse(t=100) 续跑 → 最终 hook Stop(t=1000) → 窗口。
///
/// 预期结果：
///   - 正断言：最终 Stop 从自己的时间戳起 800ms 后确认。
///   - 负断言：续跑活动之后，第一次 Stop 的窗口不得确认。
#[test]
fn active_stop_hook_still_completes_after_the_window() {
    let mut judge = judge();
    let node = 7;
    let hooked_stop = || {
        claude(
            node,
            json!({"hook_event_name": "Stop", "stop_hook_active": true, "background_tasks": []}),
        )
    };
    judge.observe(&hooked_stop(), 0);
    judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"}),
        ),
        100,
    );
    assert!(
        confirm_flags(&judge.poll(CONFIRMATION_WINDOW_MS)).is_empty(),
        "stop hook 阻止后的续跑活动必须作废第一次窗口"
    );
    judge.observe(&hooked_stop(), 1_000);
    assert_eq!(
        quiet_until_confirm(&mut judge, 1_000),
        vec![false],
        "最终那次 stop_hook_active=true 的 Stop 也必须能完成"
    );
}

/// 账本非空满 30 分钟，且回合已经 Stop，才降级确认并带标记。
///
/// 数据构造：
///   BACKGROUND_WAIT_LIMIT_MS = 30 * 60 * 1000
///   第一次 Stop 在 t=0，快照含 a、b
///   第二次 Stop 在 t=1000，快照只剩 a。时钟不重置。
///   边界：LIMIT-1 不确认，LIMIT 确认
///
/// 执行过程：两次非空 Stop，然后在两个边界 poll。
///
/// 预期结果：
///   - 正断言：t=LIMIT 的 Confirm.background_unsettled=true。
///   - 负断言：t=LIMIT-1 不确认；确认后不再重复确认。
#[test]
fn unsettled_background_work_degrades_after_thirty_minutes() {
    let mut judge = judge();
    let node = 7;
    judge.observe(&claude(node, stop_with_tasks(node, &["a", "b"])), 0);
    judge.observe(&claude(node, stop_with_tasks(node, &["a"])), 1_000);
    assert!(
        confirm_flags(&judge.poll(BACKGROUND_WAIT_LIMIT_MS - 1)).is_empty(),
        "30 分钟差 1ms 时不得超时降级"
    );
    assert_eq!(
        confirm_flags(&judge.poll(BACKGROUND_WAIT_LIMIT_MS)),
        vec![true],
        "从第一次非空 Stop 起满 30 分钟，应带未结清标记确认"
    );
    assert!(
        confirm_flags(&judge.poll(BACKGROUND_WAIT_LIMIT_MS + CONFIRMATION_WINDOW_MS)).is_empty(),
        "超时确认只发生一次"
    );
}

/// 还没有 Stop 时，即使过了 30 分钟也不提前 Done。
///
/// 数据构造：只有 PostToolUse(run_in_background)，时间跳到 LIMIT+800。
///
/// 执行过程：登记后直接 poll。
///
/// 预期结果：
///   - 负断言：poll 没有任何 Confirm。
///   - 正断言：observe 的动作里也没有 Confirm。
#[test]
fn timeout_does_not_fire_before_the_turn_stops() {
    let mut judge = judge();
    let node = 7;
    let registered = judge.observe(
        &claude(node, background_bash(node, "still", "toolu_still")),
        0,
    );
    assert!(
        confirm_flags(&registered).is_empty(),
        "登记后台任务不是完成"
    );
    assert!(
        confirm_flags(&judge.poll(BACKGROUND_WAIT_LIMIT_MS + CONFIRMATION_WINDOW_MS)).is_empty(),
        "回合还没 Stop 时，30 分钟上限不得单独把任务标完成"
    );
}

/// 账本一旦变空，30 分钟时钟清除；再次变非空后从新的 Stop 起算。
///
/// 数据构造：
///   t=0 Stop[a] 开始时钟
///   t=500 TaskStop 清空
///   t=2000 Stop[c] 重新开始
///   若时钟没清，t=LIMIT 就会确认；清了则要到 2000+LIMIT
///
/// 执行过程：按上述时间喂事件，在 LIMIT 与 2000+LIMIT 两个边界 poll。
///
/// 预期结果：
///   - 正断言：2000+LIMIT 确认且带标记。
///   - 负断言：t=LIMIT 不确认。
#[test]
fn emptying_the_ledger_restarts_the_timeout_clock() {
    let mut judge = judge();
    let node = 7;
    judge.observe(&claude(node, stop_with_tasks(node, &["a"])), 0);
    judge.observe(
        &claude(
            node,
            json!({
                "hook_event_name": "PostToolUse",
                "tool_name": "KillBash",
                "tool_input": {"task_id": "a"}
            }),
        ),
        500,
    );
    judge.observe(&claude(node, background_bash(node, "c", "toolu_c")), 1_000);
    judge.observe(&claude(node, stop_with_tasks(node, &["c"])), 2_000);
    assert!(
        confirm_flags(&judge.poll(BACKGROUND_WAIT_LIMIT_MS)).is_empty(),
        "中间清空过账本，旧的 30 分钟起点不得继续生效"
    );
    assert_eq!(
        confirm_flags(&judge.poll(2_000 + BACKGROUND_WAIT_LIMIT_MS)),
        vec![true],
        "新的非空 Stop 满 30 分钟后才超时"
    );
}

/// 进程探针发现新后代时把窗口再延 800ms，最多 3 次；晚到结果不能撤销 Done。
///
/// 数据构造：
///   UserPromptSubmit 在 t=5_000，空 Stop 在 t=5_100
///   found=true 的三次结果分别在 5_200、5_300、5_400
///   第四次在 5_500，不得超过 MAX_PROCESS_DEFERRALS=3
///   窗口终点 = 5_400+800
///
/// 执行过程：打开窗口 → 连续四次 note_process_probe → 在终点前后 poll。
///   另起一个已确认的窗口，再送 found=true。
///
/// 预期结果：
///   - 正断言：前三次各再要一次探针；终点确认。
///   - 负断言：第四次不延后；确认之后的探针不再产生 Confirm。
#[test]
fn process_probe_defers_at_most_three_windows_and_cannot_undo_done() {
    let mut judge = judge();
    let node = 7;
    judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "UserPromptSubmit", "prompt": "跑起来"}),
        ),
        5_000,
    );
    let opened = judge.observe(&claude(node, empty_stop()), 5_100);
    assert!(
        has_probe(&opened),
        "有过 UserPromptSubmit 的空 Stop 应请求一次进程探针"
    );
    for (index, at) in [5_200_u64, 5_300, 5_400].into_iter().enumerate() {
        let follow = judge.note_process_probe(node, true, at);
        assert!(
            has_probe(&follow),
            "第 {} 次发现后代应再查一次，实际 {follow:?}",
            index + 1
        );
    }
    let fourth = judge.note_process_probe(node, false, 5_500);
    assert!(fourth.is_empty(), "没发现后代时不得延后，也不得再探");
    let capped = judge.note_process_probe(node, true, 5_500);
    assert!(
        capped.is_empty(),
        "第 4 次发现后代已超过 3 次上限，窗口必须停在第三次"
    );
    assert!(
        confirm_flags(&judge.poll(5_400 + CONFIRMATION_WINDOW_MS - 1)).is_empty(),
        "第三次延后后的窗口还差 1ms"
    );
    assert_eq!(
        confirm_flags(&judge.poll(5_400 + CONFIRMATION_WINDOW_MS)),
        vec![false],
        "延后三次之后应按账本确认，探针不是完成权威"
    );

    let mut done = support::judge();
    done.observe(&claude(node, empty_stop()), 0);
    assert_eq!(
        quiet_until_confirm(&mut done, 0),
        vec![false],
        "对照窗口应已确认"
    );
    assert!(
        done.note_process_probe(node, true, 2_000).is_empty(),
        "窗口已经确认后，迟到的探针必须丢掉"
    );
    assert!(
        confirm_flags(&done.poll(2_000 + CONFIRMATION_WINDOW_MS)).is_empty(),
        "迟到探针不能重新打开已结束的回合"
    );
}

/// running 的 task-notification 不结清；空 Stop 后 36ms 的活动会取消窗口。
///
/// 数据构造：status=running 的 notification，以及空 Stop 后 36ms 的 PreToolUse。
///
/// 执行过程：登记 → running notification → 非空 Stop 不确认 → 空 Stop → PreToolUse。
///
/// 预期结果：
///   - 正断言：PreToolUse 之后需要新的空 Stop 才能确认。
///   - 负断言：running notification 后账本仍挡住完成。
#[test]
fn running_notification_and_in_window_activity_keep_the_turn_open() {
    let mut judge = judge();
    let node = 7;
    judge.observe(
        &claude(node, background_bash(node, "bd0jz0gti", "toolu_bg")),
        0,
    );
    judge.observe(
        &claude(node, notification("bd0jz0gti", "toolu_bg", "running")),
        10,
    );
    judge.observe(&claude(node, stop_with_tasks(node, &["bd0jz0gti"])), 20);
    assert!(
        confirm_flags(&judge.poll(20 + CONFIRMATION_WINDOW_MS)).is_empty(),
        "status=running 不得把后台任务从账本删掉"
    );
    judge.observe(&claude(node, empty_stop()), 100);
    judge.observe(
        &claude(
            node,
            json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"}),
        ),
        136,
    );
    assert!(
        confirm_flags(&judge.poll(100 + CONFIRMATION_WINDOW_MS)).is_empty(),
        "空 Stop 后 36ms 的 PreToolUse 必须作废确认窗口"
    );
    judge.observe(&claude(node, empty_stop()), 1_000);
    assert_eq!(
        quiet_until_confirm(&mut judge, 1_000),
        vec![false],
        "活动结束后的新空 Stop 仍可在 800ms 后确认"
    );
}
