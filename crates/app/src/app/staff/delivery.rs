pub(crate) use crate::app::agent_runtime::pending::{AGENT_ENTER_DELAY, AGENT_STARTUP_GRACE};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeliveryStage {
    Starting {
        since: Instant,
        grace_spent: bool,
    },
    Typed {
        at: Instant,
    },
    /// `retried`：确认迟迟不来时已补过一次回车；`at` 始终是第一次回车的时间。
    Submitted {
        at: Instant,
        retried: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeliveryAction {
    Wait,
    Type,
    Enter,
    NeedsYou,
}

pub(crate) const WORKER_DELIVERY_CONFIRM: Duration = Duration::from_secs(15);
/// 回车后这么久仍无 UserPromptSubmit，补一次回车（只补一次），再等到确认窗口结束。
pub(crate) const WORKER_ENTER_RETRY: Duration = Duration::from_secs(5);
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
        DeliveryStage::Submitted { at, retried } => {
            let waited = now.saturating_duration_since(at);
            if waited >= WORKER_DELIVERY_CONFIRM {
                DeliveryAction::NeedsYou
            } else if !retried && accepts_input && waited >= WORKER_ENTER_RETRY {
                DeliveryAction::Enter
            } else {
                DeliveryAction::Wait
            }
        }
    }
}

/// 会在启动时发 SessionStart（Ready）的 Agent：它们靠 Ready 放行，不能拿输出静默顶替。
/// 其余 Agent 没有可靠的 Ready 信号，仍用静默或宽限。
pub(crate) fn emits_ready_signal(profile: canvas::AgentProfile) -> bool {
    matches!(
        profile,
        canvas::AgentProfile::Claude | canvas::AgentProfile::Codex | canvas::AgentProfile::Cursor
    )
}

/// 输出静默能否当作「可以打字」：有 Ready 信号的 Agent 在尚未报告任何状态前，静默可能只是
/// 信任目录、登录或更新菜单，不算；已经报告过状态（如上一轮 Done 的续派）则静默仍可用。
pub(crate) fn quiet_allows_typing(
    profile: Option<canvas::AgentProfile>,
    reported_state: bool,
    quiet: bool,
) -> bool {
    quiet && (reported_state || !profile.is_some_and(emits_ready_signal))
}

pub(crate) fn delivery_confirmed(stage: DeliveryStage, dispatch_matches: bool) -> bool {
    dispatch_matches && matches!(stage, DeliveryStage::Submitted { .. })
}
