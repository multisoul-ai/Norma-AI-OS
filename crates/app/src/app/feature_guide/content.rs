use super::FeatureGuideTopic;

impl FeatureGuideTopic {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::AgentOrchestration => ui::i18n::text("guide.enable_agent_to_agent_delegation"),
            Self::VoiceGroupControl => ui::i18n::text("guide.demo.voice-command.title"),
            Self::AgentCollaboration => ui::i18n::text("guide.demo.agent-collaboration.title"),
            Self::LiveNodeShortcuts => ui::i18n::text("guide.demo.live-node-shortcuts.title"),
            Self::WorkspaceLayouts => ui::i18n::text("guide.demo.layout-command-m.title"),
            Self::ChiefOfStaff => ui::i18n::text("guide.demo.chief-of-staff.title"),
            Self::RightDrawer => ui::i18n::text("guide.demo.right-drawer.title"),
            Self::SwitchWorkspaces => ui::i18n::text("guide.demo.switch-workspaces.title"),
            Self::WorkspaceHome => ui::i18n::text("guide.demo.workspace-home.title"),
            Self::SlashCommands => ui::i18n::text("guide.demo.slash-commands.title"),
            Self::ShortcutCheatSheet => ui::i18n::text("guide.shortcut_cheat_sheet_title"),
        }
    }

    pub(super) fn navigation_label(self) -> &'static str {
        match self {
            Self::AgentOrchestration => ui::i18n::text("guide.agent_orchestration"),
            Self::VoiceGroupControl => ui::i18n::text("guide.demo.voice-command.label"),
            Self::AgentCollaboration => ui::i18n::text("guide.demo.agent-collaboration.label"),
            Self::LiveNodeShortcuts => ui::i18n::text("guide.demo.live-node-shortcuts.label"),
            Self::WorkspaceLayouts => ui::i18n::text("guide.demo.layout-command-m.label"),
            Self::ChiefOfStaff => ui::i18n::text("guide.demo.chief-of-staff.label"),
            Self::RightDrawer => ui::i18n::text("guide.demo.right-drawer.label"),
            Self::SwitchWorkspaces => ui::i18n::text("guide.demo.switch-workspaces.label"),
            Self::WorkspaceHome => ui::i18n::text("guide.demo.workspace-home.label"),
            Self::SlashCommands => ui::i18n::text("guide.demo.slash-commands.label"),
            Self::ShortcutCheatSheet => ui::i18n::text("guide.shortcut_cheat_sheet"),
        }
    }

    pub(super) fn eyebrow(self) -> &'static str {
        match self {
            Self::AgentOrchestration => ui::i18n::text("guide.agent_orchestration_2"),
            Self::VoiceGroupControl => ui::i18n::text("guide.demo.voice-command.label"),
            Self::AgentCollaboration => ui::i18n::text("guide.demo.agent-collaboration.label"),
            Self::LiveNodeShortcuts => ui::i18n::text("guide.demo.live-node-shortcuts.label"),
            Self::WorkspaceLayouts => ui::i18n::text("guide.demo.layout-command-m.label"),
            Self::ChiefOfStaff => ui::i18n::text("guide.demo.chief-of-staff.label"),
            Self::RightDrawer => ui::i18n::text("guide.demo.right-drawer.label"),
            Self::SwitchWorkspaces => ui::i18n::text("guide.demo.switch-workspaces.label"),
            Self::WorkspaceHome => ui::i18n::text("guide.demo.workspace-home.label"),
            Self::SlashCommands => ui::i18n::text("guide.demo.slash-commands.label"),
            Self::ShortcutCheatSheet => ui::i18n::text("guide.shortcut_cheat_sheet_eyebrow"),
        }
    }

    pub(super) fn summary(self) -> &'static str {
        match self {
            Self::AgentOrchestration => ui::i18n::text(
                "guide.install_orchestration_for_the_agents_you_use_norma_configures_the_skill",
            ),
            Self::VoiceGroupControl => ui::i18n::text("guide.demo.voice-command.summary"),
            Self::AgentCollaboration => ui::i18n::text("guide.demo.agent-collaboration.summary"),
            Self::LiveNodeShortcuts => ui::i18n::text("guide.demo.live-node-shortcuts.summary"),
            Self::WorkspaceLayouts => ui::i18n::text("guide.demo.layout-command-m.summary"),
            Self::ChiefOfStaff => ui::i18n::text("guide.demo.chief-of-staff.summary"),
            Self::RightDrawer => ui::i18n::text("guide.demo.right-drawer.summary"),
            Self::SwitchWorkspaces => ui::i18n::text("guide.demo.switch-workspaces.summary"),
            Self::WorkspaceHome => ui::i18n::text("guide.demo.workspace-home.summary"),
            Self::SlashCommands => ui::i18n::text("guide.demo.slash-commands.summary"),
            Self::ShortcutCheatSheet => ui::i18n::text("guide.shortcut_cheat_sheet_summary"),
        }
    }

    pub(super) fn video_asset(self) -> Option<&'static str> {
        match self {
            Self::AgentOrchestration | Self::ShortcutCheatSheet => None,
            Self::VoiceGroupControl => Some("feature-guide/demo-voice-command.mp4"),
            Self::AgentCollaboration => Some("feature-guide/demo-agent-collaboration.mp4"),
            Self::LiveNodeShortcuts => Some("feature-guide/demo-live-node-shortcuts.mp4"),
            Self::WorkspaceLayouts => Some("feature-guide/demo-layout-command-m.mp4"),
            Self::ChiefOfStaff => Some("feature-guide/demo-chief-of-staff.mp4"),
            Self::RightDrawer => Some("feature-guide/demo-right-drawer.mp4"),
            Self::SwitchWorkspaces => Some("feature-guide/demo-switch-workspaces.mp4"),
            Self::WorkspaceHome => Some("feature-guide/demo-workspace-home.mp4"),
            Self::SlashCommands => Some("feature-guide/demo-slash-commands.mp4"),
        }
    }

    pub(super) fn poster_asset(self) -> Option<&'static str> {
        match self {
            Self::AgentOrchestration | Self::ShortcutCheatSheet => None,
            Self::VoiceGroupControl => Some("feature-guide/demo-voice-command.webp"),
            Self::AgentCollaboration => Some("feature-guide/demo-agent-collaboration.webp"),
            Self::LiveNodeShortcuts => Some("feature-guide/demo-live-node-shortcuts.webp"),
            Self::WorkspaceLayouts => Some("feature-guide/demo-layout-command-m.webp"),
            Self::ChiefOfStaff => Some("feature-guide/demo-chief-of-staff.webp"),
            Self::RightDrawer => Some("feature-guide/demo-right-drawer.webp"),
            Self::SwitchWorkspaces => Some("feature-guide/demo-switch-workspaces.webp"),
            Self::WorkspaceHome => Some("feature-guide/demo-workspace-home.webp"),
            Self::SlashCommands => Some("feature-guide/demo-slash-commands.webp"),
        }
    }
}
