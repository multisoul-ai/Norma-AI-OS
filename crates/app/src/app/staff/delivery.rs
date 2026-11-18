pub(crate) use crate::app::agent_runtime::pending::AGENT_STARTUP_GRACE;
use std::time::{Duration, Instant};

/// 首次 PTY 输出后静默多久才可打字。短于 30 秒启动宽限，长于约 12 毫秒一帧的 ANSI 重绘。
pub(crate) const AGENT_OUTPUT_QUIET: Duration = Duration::from_millis(1500);

/// 启动阶段能否打字。Ready、输出静默、重新投递或 30 秒宽限，并且卡片接收输入、dispatch 对得上。
pub(crate) fn startup_can_type(
    elapsed: Duration,
    grace_spent: bool,
    ready: bool,
    quiet: bool,
    accepts_input: bool,
    dispatch_matches: bool,
) -> bool {
    dispatch_matches
        && accepts_input
        && (ready || quiet || grace_spent || elapsed >= AGENT_STARTUP_GRACE)
}

pub(crate) fn output_is_quiet(last_output_at: Option<Instant>, now: Instant) -> bool {
    last_output_at.is_some_and(|at| now.saturating_duration_since(at) >= AGENT_OUTPUT_QUIET)
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
