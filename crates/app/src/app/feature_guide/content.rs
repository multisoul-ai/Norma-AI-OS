use super::FeatureGuideTopic;

impl FeatureGuideTopic {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::AgentOrchestration => "Enable Agent-to-Agent Delegation",
            Self::VoiceGroupControl => "Voice group control: direct six Agents at once",
            Self::AgentCollaboration => "Agents delegate tasks to each other",
            Self::LiveNodeShortcuts => "Jump to Any Live Node",
            Self::WorkspaceLayouts => "Reflow Your Workspace",
            Self::AgentNotifications => "Know When an Agent Needs You",
        }
    }

    pub(super) fn navigation_label(self) -> &'static str {
        match self {
            Self::AgentOrchestration => "Agent Orchestration",
            Self::VoiceGroupControl => "Voice Group Control",
            Self::AgentCollaboration => "Agent Collaboration",
            Self::LiveNodeShortcuts => "Live Node Shortcuts",
            Self::WorkspaceLayouts => "Workspace Layouts",
            Self::AgentNotifications => "Agent Notifications",
        }
    }

    pub(super) fn eyebrow(self) -> &'static str {
        match self {
            Self::AgentOrchestration => "AGENT ORCHESTRATION",
            Self::VoiceGroupControl => "VOICE GROUP CONTROL",
            Self::AgentCollaboration => "AGENT COLLABORATION",
            Self::LiveNodeShortcuts => "LIVE NODE SHORTCUTS",
            Self::WorkspaceLayouts => "SPATIAL WORKSPACE",
            Self::AgentNotifications => "AGENT NOTIFICATIONS",
        }
    }

    pub(super) fn summary(self) -> &'static str {
        match self {
            Self::AgentOrchestration => "Install orchestration for the agents you use. Norma configures the Skill, CLI, Agent Hook, and authorization together.",
            Self::VoiceGroupControl => "One spoken instruction controls six Agents. Norma creates six Agent Live Nodes, gives each one its own task, and keeps every run visible in the same workspace.",
            Self::AgentCollaboration => "One Agent can hand a focused task to another Agent and receive the result inside the same workspace. Work branches without making you relay every instruction yourself.",
            Self::LiveNodeShortcuts => "Press ⌘1–6 to bring a specific Live Node forward instantly. Move between agents, terminals, and previews without hunting through tabs.",
            Self::WorkspaceLayouts => "Press ⌘M to switch between focused and overview layouts. Every running Live Node stays visible, active, and connected to its task.",
            Self::AgentNotifications => "Completion and attention indicators appear directly on the relevant Live Node, so you can see which agent changed state without checking every terminal.",
        }
    }

    pub(super) fn video_asset(self) -> Option<&'static str> {
        match self {
            Self::AgentOrchestration => None,
            Self::VoiceGroupControl => Some("feature-guide/demo-voice-command.mp4"),
            Self::AgentCollaboration => Some("feature-guide/demo-agent-collaboration.mp4"),
            Self::LiveNodeShortcuts => Some("feature-guide/demo-live-node-shortcuts.mp4"),
            Self::WorkspaceLayouts => Some("feature-guide/demo-layout-command-m.mp4"),
            Self::AgentNotifications => Some("feature-guide/demo-agent-notifications.mp4"),
        }
    }

    pub(super) fn poster_asset(self) -> Option<&'static str> {
        match self {
            Self::AgentOrchestration => None,
            Self::VoiceGroupControl => Some("feature-guide/demo-voice-command.webp"),
            Self::AgentCollaboration => Some("feature-guide/demo-agent-collaboration.webp"),
            Self::LiveNodeShortcuts => Some("feature-guide/demo-live-node-shortcuts.webp"),
            Self::WorkspaceLayouts => Some("feature-guide/demo-layout-command-m.webp"),
            Self::AgentNotifications => Some("feature-guide/demo-agent-notifications.webp"),
        }
    }
}
