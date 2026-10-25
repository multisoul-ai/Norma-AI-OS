use agent::{AgentRosterEntry, DesktopContext, DirectPlan};

#[cfg(test)]
#[path = "../../../../tests/unit/doubao_direct_address_tests.rs"]
mod tests;

/// 桌面工作空间（总览/项目画布/切换中）使用跨项目工具，不走单项目直呼路由。
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

/// 与指令模式同一套确定性路由；未命中（`Skip`）时调用方回落到模型。
pub(super) fn plan(
    transcript: &str,
    roster: &[AgentRosterEntry],
    desktop: Option<&DesktopContext>,
) -> DirectPlan {
    if desktop_active(desktop) {
        return DirectPlan::Skip;
    }
    agent::plan_direct_address(transcript, roster)
}
