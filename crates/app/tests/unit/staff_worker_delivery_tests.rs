use super::{
    delivery_action, delivery_confirmed, output_is_quiet, DeliveryAction, DeliveryStage,
    AGENT_ENTER_DELAY, AGENT_OUTPUT_QUIET, AGENT_STARTUP_GRACE, WORKER_DELIVERY_CONFIRM,
};
use std::time::{Duration, Instant};

fn starting(since: Instant, grace_spent: bool) -> DeliveryStage {
    DeliveryStage::Starting { since, grace_spent }
}

/// 无 Ready 的 Agent 等到共享 30 秒宽限才打字；Ready 或输出静默可以提前。
///
/// 数据构造：起点 now，宽限 30 秒（与 AGENT_STARTUP_GRACE 同一常量，不另写数值）。
/// 静默阈值 1500 毫秒：短于 30 秒宽限，长于约 12 毫秒一帧。
/// 执行过程：29 秒、30 秒、Ready、静默、宽限已标记、卡片拒收输入、dispatch 不匹配各判一次。
/// 预期结果：只有宽限到达、Ready、静默或重新投递，且卡片能接收输入时才是 Type；不匹配或拒收保持 Wait。
#[test]
fn startup_types_on_grace_ready_or_quiet_output() {
    let now = Instant::now();
    let stage = starting(now, false);
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
    assert_eq!(
        delivery_action(
            stage,
            now + Duration::from_secs(29),
            false,
            false,
            true,
            true
        ),
        DeliveryAction::Wait,
        "29 seconds is still inside the 30 second grace"
    );
    assert_eq!(
        delivery_action(
            stage,
            now + Duration::from_secs(30),
            false,
            false,
            true,
            true
        ),
        DeliveryAction::Type,
        "OpenCode, ZCode and Kimi type once the 30 second grace elapses"
    );
    assert_eq!(
        delivery_action(stage, now, true, false, true, true),
        DeliveryAction::Type,
        "a Ready hook types immediately"
    );
    assert_eq!(
        delivery_action(stage, now, false, true, true, true),
        DeliveryAction::Type,
        "quiet PTY output types immediately"
    );
    assert_eq!(
        delivery_action(starting(now, true), now, false, false, true, true),
        DeliveryAction::Type,
        "redelivery spends the grace and types on the next tick"
    );
    assert_eq!(
        delivery_action(
            stage,
            now + Duration::from_secs(30),
            true,
            true,
            true,
            false
        ),
        DeliveryAction::Wait,
        "a card that still holds input waits instead of failing the worker"
    );
    assert_eq!(
        delivery_action(
            stage,
            now + Duration::from_secs(30),
            true,
            true,
            false,
            true
        ),
        DeliveryAction::Wait,
        "a different dispatch must not receive this prompt"
    );
    assert_ne!(
        delivery_action(
            stage,
            now + Duration::from_secs(29),
            false,
            false,
            true,
            true
        ),
        DeliveryAction::Type,
        "grace must not fire one second early"
    );
}

/// Enter 后只等本轮送达；15 秒没有 UserPromptSubmit 就转为需要你。
///
/// 数据构造：Typed 与 Submitted 的起点都是 now。Enter 延迟 100 毫秒，确认窗口 15 秒。
/// 执行过程：Typed 未满 100 毫秒、满 100 毫秒、满 100 毫秒但拒收输入、Submitted 14 秒、15 秒且拒收输入，以及各阶段的送达判断。
/// 预期结果：未满延迟或卡片拒收继续等，到点且能接收输入才按 Enter；14 秒仍等，15 秒 NeedsYou 且不再重试；只有 Submitted 且 dispatch 匹配才算出送达。
#[test]
fn enter_then_delivery_timeout_needs_the_user() {
    let now = Instant::now();
    let typed = DeliveryStage::Typed { at: now };
    let submitted = DeliveryStage::Submitted { at: now };
    assert_eq!(
        AGENT_ENTER_DELAY,
        Duration::from_millis(100),
        "Enter delay must stay the shared 100 millisecond rule"
    );
    assert_eq!(
        WORKER_DELIVERY_CONFIRM,
        Duration::from_secs(15),
        "delivery confirmation stays 15 seconds"
    );
    assert_eq!(
        delivery_action(
            typed,
            now + Duration::from_millis(99),
            false,
            false,
            true,
            true
        ),
        DeliveryAction::Wait,
        "paste needs 100 milliseconds before Enter"
    );
    assert_eq!(
        delivery_action(
            typed,
            now + Duration::from_millis(100),
            false,
            false,
            true,
            true
        ),
        DeliveryAction::Enter,
        "Enter is sent once the shared delay elapses"
    );
    assert_eq!(
        delivery_action(
            typed,
            now + Duration::from_millis(100),
            false,
            false,
            true,
            false
        ),
        DeliveryAction::Wait,
        "Enter waits while the card is not accepting input"
    );
    assert_eq!(
        delivery_action(
            submitted,
            now + Duration::from_secs(14),
            false,
            false,
            true,
            true
        ),
        DeliveryAction::Wait,
        "14 seconds is still inside the 15 second confirmation window"
    );
    assert_eq!(
        delivery_action(
            submitted,
            now + Duration::from_secs(15),
            true,
            true,
            true,
            false
        ),
        DeliveryAction::NeedsYou,
        "15 seconds without UserPromptSubmit asks the user and does not retry"
    );
    assert!(
        delivery_confirmed(submitted, true),
        "UserPromptSubmit confirms only the submitted round"
    );
    assert!(
        !delivery_confirmed(submitted, false),
        "another node's submit does not confirm this worker"
    );
    assert!(
        !delivery_confirmed(typed, true),
        "a submit that arrives before Enter is not this round"
    );
    assert!(
        !delivery_confirmed(starting(now, false), true),
        "startup output is not delivery"
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
