use super::FeatureGuideTopic;

impl FeatureGuideTopic {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::AgentOrchestration => ui::i18n::text("guide.enable_agent_to_agent_delegation"),
            Self::VoiceGroupControl => {
                ui::i18n::text("guide.voice_group_control_direct_six_agents_at_once")
            }
            Self::AgentCollaboration => ui::i18n::text("guide.agents_delegate_tasks_to_each_other"),
            Self::LiveNodeShortcuts => ui::i18n::text("guide.jump_to_any_live_node"),
            Self::WorkspaceLayouts => ui::i18n::text("guide.reflow_your_workspace"),
            Self::AgentNotifications => ui::i18n::text("guide.know_when_an_agent_needs_you"),
            Self::ShortcutCheatSheet => ui::i18n::text("guide.shortcut_cheat_sheet_title"),
        }
    }

    pub(super) fn navigation_label(self) -> &'static str {
        match self {
            Self::AgentOrchestration => ui::i18n::text("guide.agent_orchestration"),
            Self::VoiceGroupControl => ui::i18n::text("guide.voice_group_control"),
            Self::AgentCollaboration => ui::i18n::text("guide.agent_collaboration"),
            Self::LiveNodeShortcuts => ui::i18n::text("guide.live_node_shortcuts"),
            Self::WorkspaceLayouts => ui::i18n::text("guide.workspace_layouts"),
            Self::AgentNotifications => ui::i18n::text("guide.agent_notifications"),
            Self::ShortcutCheatSheet => ui::i18n::text("guide.shortcut_cheat_sheet"),
        }
    }

    pub(super) fn eyebrow(self) -> &'static str {
        match self {
            Self::AgentOrchestration => ui::i18n::text("guide.agent_orchestration_2"),
            Self::VoiceGroupControl => ui::i18n::text("guide.voice_group_control_3"),
            Self::AgentCollaboration => ui::i18n::text("guide.agent_collaboration_2"),
            Self::LiveNodeShortcuts => ui::i18n::text("guide.live_node_shortcuts_2"),
            Self::WorkspaceLayouts => ui::i18n::text("guide.spatial_workspace"),
            Self::AgentNotifications => ui::i18n::text("guide.agent_notifications_2"),
            Self::ShortcutCheatSheet => ui::i18n::text("guide.shortcut_cheat_sheet_eyebrow"),
        }
    }

    pub(super) fn summary(self) -> &'static str {
        match self {
            Self::AgentOrchestration => ui::i18n::text("guide.install_orchestration_for_the_agents_you_use_norma_configures_the_skill"),
            Self::VoiceGroupControl => ui::i18n::text("guide.one_spoken_instruction_controls_six_agents_norma_creates_six_agent_live"),
            Self::AgentCollaboration => ui::i18n::text("guide.one_agent_can_hand_a_focused_task_to_another_agent_and"),
            Self::LiveNodeShortcuts => ui::i18n::text("guide.press_1_6_to_bring_a_specific_live_node_forward_instantly"),
            Self::WorkspaceLayouts => ui::i18n::text("guide.press_m_to_switch_between_focused_and_overview_layouts_every_running"),
            Self::AgentNotifications => ui::i18n::text("guide.completion_and_attention_indicators_appear_directly_on_the_relevant_live_node"),
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
            Self::AgentNotifications => Some("feature-guide/demo-agent-notifications.mp4"),
        }
    }

    pub(super) fn poster_asset(self) -> Option<&'static str> {
        match self {
            Self::AgentOrchestration | Self::ShortcutCheatSheet => None,
            Self::VoiceGroupControl => Some("feature-guide/demo-voice-command.webp"),
            Self::AgentCollaboration => Some("feature-guide/demo-agent-collaboration.webp"),
            Self::LiveNodeShortcuts => Some("feature-guide/demo-live-node-shortcuts.webp"),
            Self::WorkspaceLayouts => Some("feature-guide/demo-layout-command-m.webp"),
            Self::AgentNotifications => Some("feature-guide/demo-agent-notifications.webp"),
        }
    }
}
