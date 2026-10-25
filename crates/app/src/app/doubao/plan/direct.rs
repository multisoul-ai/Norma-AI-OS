use agent::{AgentRosterEntry, DesktopContext, DirectPlan};

#[cfg(test)]
#[path = "../../../../tests/unit/doubao_direct_address_tests.rs"]
mod tests;

/// 桌面工作空间（总览/项目画布/切换中）的跨项目工具过滤口径，与直呼门控无关。
pub(super) fn desktop_active(desktop: Option<&DesktopContext>) -> bool {
    matches!(
        desktop,
        Some(
            DesktopContext::WorkspaceOverview { .. }
                | DesktopContext::ProjectCanvas { .. }
                | DesktopContext::Switching { .. }
        )
    )
}

/// 总览没有单一项目、切换中名册不稳定，这两种桌面状态不能做确定性直呼；
/// 项目画布与独立画布的名册都属于当前项目，必须直呼。
fn direct_route_blocked(desktop: Option<&DesktopContext>) -> bool {
    matches!(
        desktop,
        Some(DesktopContext::WorkspaceOverview { .. } | DesktopContext::Switching { .. })
    )
}

/// 与指令模式同一套确定性路由；未命中（`Skip`）时调用方回落到模型。
pub(super) fn plan(
    transcript: &str,
    roster: &[AgentRosterEntry],
    desktop: Option<&DesktopContext>,
) -> DirectPlan {
    if direct_route_blocked(desktop) {
        return DirectPlan::Skip;
    }
    agent::plan_direct_address(transcript, roster)
}
