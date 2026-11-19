use super::{
    emits_ready_signal, output_is_quiet, quiet_allows_typing, startup_can_type, AGENT_OUTPUT_QUIET,
    AGENT_STARTUP_GRACE,
};
use canvas::AgentProfile;
use std::time::{Duration, Instant};

/// 无 Ready 的 Agent 等到共享 30 秒宽限才打字；Ready 或输出静默可以提前。
///
/// 数据构造：宽限 30 秒（与 AGENT_STARTUP_GRACE 同一常量）。静默阈值 1500 毫秒。
/// 执行过程：29 秒、30 秒、Ready、静默、宽限已标记、卡片拒收输入、dispatch 不匹配各判一次。
/// 预期结果：只有宽限到达、Ready、静默或重新投递，且卡片能接收输入、dispatch 匹配时才可打字。
#[test]
fn startup_types_on_grace_ready_or_quiet_output() {
    assert_eq!(
        AGENT_STARTUP_GRACE,
        Duration::from_secs(30),
        "startup grace must stay the shared 30 second rule"
    );
    assert_eq!(
        AGENT_OUTPUT_QUIET,
        Duration::from_millis(1500),
        "quiet window is 1.5 seconds after the first PTY output"
    );
    assert!(
        !startup_can_type(Duration::from_secs(29), false, false, false, true, true),
        "29 seconds is still inside the 30 second grace"
    );
    assert!(
        startup_can_type(Duration::from_secs(30), false, false, false, true, true),
        "OpenCode, ZCode and Kimi type once the 30 second grace elapses"
    );
    assert!(
        startup_can_type(Duration::ZERO, false, true, false, true, true),
        "a Ready hook types immediately"
    );
    assert!(
        startup_can_type(Duration::ZERO, false, false, true, true, true),
        "quiet PTY output types immediately"
    );
    assert!(
        startup_can_type(Duration::ZERO, true, false, false, true, true),
        "redelivery spends the grace and types on the next tick"
    );
    assert!(
        !startup_can_type(Duration::from_secs(30), false, true, true, false, true),
        "a card that still holds input waits instead of failing the worker"
    );
    assert!(
        !startup_can_type(Duration::from_secs(30), false, true, true, true, false),
        "a different dispatch must not receive this prompt"
    );
}

/// 从未输出不算静默；首次输出后满 1.5 秒才可打字。
///
/// 数据构造：没有时间戳，以及距离 now 1499 毫秒和 1500 毫秒的两次输出。
/// 执行过程：分别调用 output_is_quiet。
/// 预期结果：缺时间戳和 1499 毫秒都不是静默，1500 毫秒是静默。
#[test]
fn quiet_requires_output_then_one_and_a_half_seconds() {
    let now = Instant::now();
    assert!(
        !output_is_quiet(None, now),
        "a card that has never printed is not quiet"
    );
    assert!(
        !output_is_quiet(Some(now - Duration::from_millis(1499)), now),
        "1499 milliseconds is still inside the 1500 millisecond quiet window"
    );
    assert!(
        output_is_quiet(Some(now - Duration::from_millis(1500)), now),
        "1500 milliseconds after the last PTY output is quiet enough to type"
    );
}

/// 有 Ready 信号的 Agent 不能靠静默放行；没有 Ready 信号的仍可以。
///
/// 数据构造：Claude/Codex/Cursor 有 Ready；OpenCode/Kimi/ZCode 没有；quiet 为真。
/// 执行过程：分别在“尚未报告任何状态”和“已报告状态（续派）”下判断静默是否放行。
/// 预期结果：有 Ready 的 Agent 未报告状态时不放行，已报告状态或无 Ready 的 Agent 放行；quiet 为假永不放行；未知卡片按无 Ready 处理。
#[test]
fn quiet_output_does_not_replace_a_ready_signal() {
    for profile in [
        AgentProfile::Claude,
        AgentProfile::Codex,
        AgentProfile::Cursor,
    ] {
        assert!(
            emits_ready_signal(profile),
            "{profile:?} reports SessionStart"
        );
        assert!(
            !quiet_allows_typing(Some(profile), false, true),
            "{profile:?} must wait for Ready"
        );
        assert!(
            quiet_allows_typing(Some(profile), true, true),
            "{profile:?} already reported, quiet is fine"
        );
        assert!(
            !quiet_allows_typing(Some(profile), true, false),
            "no quiet, no pass"
        );
    }
    for profile in [
        AgentProfile::OpenCode,
        AgentProfile::Kimi,
        AgentProfile::ZCode,
    ] {
        assert!(
            !emits_ready_signal(profile),
            "{profile:?} has no dependable Ready"
        );
        assert!(
            quiet_allows_typing(Some(profile), false, true),
            "{profile:?} keeps the quiet fallback"
        );
    }
    assert!(
        quiet_allows_typing(None, false, true),
        "unknown card keeps the quiet fallback"
    );
}
