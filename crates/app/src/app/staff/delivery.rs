pub(crate) use crate::app::agent_runtime::pending::{AGENT_ENTER_DELAY, AGENT_STARTUP_GRACE};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeliveryStage {
    Starting { since: Instant, grace_spent: bool },
    Typed { at: Instant },
    Submitted { at: Instant },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeliveryAction {
    Wait,
    Type,
    Enter,
    NeedsYou,
}

pub(crate) const WORKER_DELIVERY_CONFIRM: Duration = Duration::from_secs(15);
/// 首次 PTY 输出后静默多久才可打字。短于 30 秒启动宽限，长于约 12 毫秒一帧的 ANSI 重绘。
pub(crate) const AGENT_OUTPUT_QUIET: Duration = Duration::from_millis(1500);

pub(crate) fn output_is_quiet(last_output_at: Option<Instant>, now: Instant) -> bool {
    last_output_at.is_some_and(|at| now.saturating_duration_since(at) >= AGENT_OUTPUT_QUIET)
}

pub(crate) fn delivery_action(
    stage: DeliveryStage,
    now: Instant,
    ready: bool,
    quiet: bool,
    dispatch_matches: bool,
    accepts_input: bool,
) -> DeliveryAction {
    if !dispatch_matches {
        return DeliveryAction::Wait;
    }
    match stage {
        DeliveryStage::Starting { since, grace_spent } => {
            let grace = grace_spent || now.saturating_duration_since(since) >= AGENT_STARTUP_GRACE;
            if accepts_input && (ready || quiet || grace) {
                DeliveryAction::Type
            } else {
                DeliveryAction::Wait
            }
        }
        DeliveryStage::Typed { at } => {
            if accepts_input && now.saturating_duration_since(at) >= AGENT_ENTER_DELAY {
                DeliveryAction::Enter
            } else {
                DeliveryAction::Wait
            }
        }
        DeliveryStage::Submitted { at } => {
            if now.saturating_duration_since(at) >= WORKER_DELIVERY_CONFIRM {
                DeliveryAction::NeedsYou
            } else {
                DeliveryAction::Wait
            }
        }
    }
}

pub(crate) fn delivery_confirmed(stage: DeliveryStage, dispatch_matches: bool) -> bool {
    dispatch_matches && matches!(stage, DeliveryStage::Submitted { .. })
}
