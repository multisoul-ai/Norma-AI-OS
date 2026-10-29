//! 启动看门狗的武装集合：哪些 Agent 在启动时会发出 SessionStart，且其 hook 已安装。
use crate::app::MultisoulApp;
use canvas::AgentProfile;
use norma::agent_hook::{agent_hooks_installed, default_install_paths, HookTarget};

/// 仅返回「启动即发 SessionStart」已有文档证据的 profile 对应的 hook 目标。
///
/// 不在此列的 CLI 要么懒触发（Kimi、ZCode 在首条消息才发），要么没有启动期证据
/// （Cursor、Grok、OpenCode、InfCode），武装它们会让健康的新卡片误报。
pub(crate) fn startup_signal_target(profile: AgentProfile) -> Option<HookTarget> {
    match profile {
        AgentProfile::Codex => Some(HookTarget::Codex),
        AgentProfile::Claude => Some(HookTarget::Claude),
        AgentProfile::Pi => Some(HookTarget::Pi),
        AgentProfile::Hermes => Some(HookTarget::Hermes),
        AgentProfile::Cursor
        | AgentProfile::Grok
        | AgentProfile::OpenCode
        | AgentProfile::InfCode
        | AgentProfile::Kimi
        | AgentProfile::ZCode => None,
    }
}

const ALL_PROFILES: [AgentProfile; 10] = [
    AgentProfile::Claude,
    AgentProfile::Codex,
    AgentProfile::Cursor,
    AgentProfile::Grok,
    AgentProfile::OpenCode,
    AgentProfile::InfCode,
    AgentProfile::Kimi,
    AgentProfile::ZCode,
    AgentProfile::Pi,
    AgentProfile::Hermes,
];

pub(crate) fn armed_startup_profiles(
    is_installed: impl Fn(HookTarget) -> bool,
) -> Vec<AgentProfile> {
    ALL_PROFILES
        .into_iter()
        .filter(|profile| startup_signal_target(*profile).is_some_and(&is_installed))
        .collect()
}

impl MultisoulApp {
    /// 读几个小配置文件；只在工作区注册与 hook 开关变更时调用，不得进入帧循环。
    pub(crate) fn refresh_startup_watch_profiles(&mut self) {
        let paths = default_install_paths();
        let profiles = armed_startup_profiles(|target| agent_hooks_installed(target, &paths));
        self.runtime.set_startup_watch_profiles(profiles);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 武装集合只含「启动即发 SessionStart」且 hook 已安装的 profile。
    ///
    /// 数据构造：10 个 profile；安装检测桩把 Codex、Cursor、Kimi 报告为已安装，其余未装。
    ///
    /// 执行过程：armed_startup_profiles(stub)
    ///   - Codex：有启动信号目标且已装 → 入选
    ///   - Cursor：已装但无启动期证据 → 排除
    ///   - Kimi：已装但 SessionStart 懒触发 → 排除
    ///   - Claude/Pi/Hermes：有目标但未装 → 排除
    ///
    /// 预期结果：恰好 [Codex]。
    #[test]
    fn only_installed_launch_signal_profiles_are_armed() {
        let armed = armed_startup_profiles(|target| {
            matches!(
                target,
                HookTarget::Codex | HookTarget::Cursor | HookTarget::Kimi
            )
        });

        assert_eq!(
            armed,
            vec![AgentProfile::Codex],
            "只有已装且启动即发信号的 Codex 应入选"
        );
    }

    /// 全部都已安装时，懒触发与无证据的 profile 仍被排除。
    ///
    /// 数据构造：安装检测桩对所有目标返回 true。
    /// 预期结果：恰为 Claude、Codex、Pi、Hermes 四个；Kimi/ZCode（懒触发）及
    /// Cursor/Grok/OpenCode/InfCode（无证据）均不在其中。
    #[test]
    fn lazy_or_unverified_profiles_are_never_armed() {
        let armed = armed_startup_profiles(|_| true);

        assert_eq!(armed.len(), 4, "只应武装 4 个有启动期证据的 profile");
        for expected in [
            AgentProfile::Claude,
            AgentProfile::Codex,
            AgentProfile::Pi,
            AgentProfile::Hermes,
        ] {
            assert!(armed.contains(&expected), "{expected:?} 应被武装");
        }
        for excluded in [
            AgentProfile::Kimi,
            AgentProfile::ZCode,
            AgentProfile::Cursor,
            AgentProfile::Grok,
            AgentProfile::OpenCode,
            AgentProfile::InfCode,
        ] {
            assert!(
                !armed.contains(&excluded),
                "{excluded:?} 不得被武装，否则健康新卡片会误报"
            );
        }
    }
}
