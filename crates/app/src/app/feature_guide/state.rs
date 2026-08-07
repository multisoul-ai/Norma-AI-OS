#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FeatureGuideStatus {
    NotSeen,
    InProgress,
    Completed,
    Suppressed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FeatureGuidePresentation {
    Idle,
    Pending,
    Visible,
    Suspended,
    ClosedForSession,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum FeatureGuideSettingsTransition {
    #[default]
    None,
    Opening,
    Closing,
}

pub(crate) fn settings_crossfade_opacities(
    transition: FeatureGuideSettingsTransition,
    progress: f32,
) -> (f32, f32) {
    let progress = progress.clamp(0.0, 1.0);
    match transition {
        FeatureGuideSettingsTransition::Opening => (1.0 - progress, progress),
        FeatureGuideSettingsTransition::Closing => (progress, 1.0 - progress),
        FeatureGuideSettingsTransition::None => (1.0, 1.0),
    }
}

pub(crate) fn feature_guide_crossfade_opacity(
    transition: FeatureGuideSettingsTransition,
    progress: f32,
) -> f32 {
    settings_crossfade_opacities(transition, progress).0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FeatureGuideTopic {
    AgentOrchestration,
    VoiceGroupControl,
    AgentCollaboration,
    LiveNodeShortcuts,
    WorkspaceLayouts,
    AgentNotifications,
}

impl FeatureGuideTopic {
    pub(crate) const ALL: [Self; 6] = [
        Self::AgentOrchestration,
        Self::VoiceGroupControl,
        Self::AgentCollaboration,
        Self::LiveNodeShortcuts,
        Self::WorkspaceLayouts,
        Self::AgentNotifications,
    ];

    fn from_saved(value: &str) -> Option<Self> {
        match value {
            "agent_orchestration" => Some(Self::AgentOrchestration),
            "voice_group_control" => Some(Self::VoiceGroupControl),
            "agent_collaboration" => Some(Self::AgentCollaboration),
            "live_node_shortcuts" => Some(Self::LiveNodeShortcuts),
            "workspace_layouts" => Some(Self::WorkspaceLayouts),
            "agent_notifications" => Some(Self::AgentNotifications),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FeatureGuideBlockers {
    pub(crate) settings_open: bool,
    pub(crate) voice_active: bool,
    pub(crate) confirmation_open: bool,
    pub(crate) workspace_error_visible: bool,
}

impl FeatureGuideBlockers {
    pub(crate) fn any(self) -> bool {
        self.settings_open
            || self.voice_active
            || self.confirmation_open
            || self.workspace_error_visible
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FeatureGuideState {
    pub(crate) status: FeatureGuideStatus,
    pub(crate) presentation: FeatureGuidePresentation,
    pub(crate) topic: FeatureGuideTopic,
    auto_eligible: bool,
    orchestration_details_expanded: bool,
    settings_transition: FeatureGuideSettingsTransition,
    settings_transition_generation: u64,
}

impl FeatureGuideState {
    pub(crate) fn load(
        saved_status: Option<&str>,
        saved_topic: Option<&str>,
        had_recent_workspaces: bool,
    ) -> Self {
        let parsed_status = saved_status.and_then(|value| match value {
            "not_seen" => Some(FeatureGuideStatus::NotSeen),
            "in_progress" => Some(FeatureGuideStatus::InProgress),
            "completed" => Some(FeatureGuideStatus::Completed),
            "suppressed" => Some(FeatureGuideStatus::Suppressed),
            _ => None,
        });
        let status = parsed_status.unwrap_or(FeatureGuideStatus::NotSeen);
        let auto_eligible = match parsed_status {
            Some(FeatureGuideStatus::NotSeen | FeatureGuideStatus::InProgress) => true,
            Some(FeatureGuideStatus::Completed | FeatureGuideStatus::Suppressed) => false,
            None => !had_recent_workspaces,
        };
        Self {
            status,
            presentation: FeatureGuidePresentation::Idle,
            topic: saved_topic
                .and_then(FeatureGuideTopic::from_saved)
                .unwrap_or(FeatureGuideTopic::AgentOrchestration),
            auto_eligible,
            orchestration_details_expanded: false,
            settings_transition: FeatureGuideSettingsTransition::None,
            settings_transition_generation: 0,
        }
    }

    pub(crate) fn on_workspace_opened(&mut self) {
        if self.auto_eligible && self.presentation != FeatureGuidePresentation::ClosedForSession {
            self.presentation = FeatureGuidePresentation::Pending;
        }
    }

    pub(crate) fn on_canvas_ready(&mut self, blockers: FeatureGuideBlockers) {
        if self.presentation == FeatureGuidePresentation::Pending && !blockers.any() {
            self.presentation = FeatureGuidePresentation::Visible;
            self.status = FeatureGuideStatus::InProgress;
        }
    }

    pub(crate) fn sync_blockers(&mut self, blockers: FeatureGuideBlockers) {
        match (self.presentation, blockers.any()) {
            (FeatureGuidePresentation::Pending, false) => {
                self.presentation = FeatureGuidePresentation::Visible;
                self.status = FeatureGuideStatus::InProgress;
            }
            (FeatureGuidePresentation::Visible, true) => {
                self.presentation = FeatureGuidePresentation::Suspended;
            }
            (FeatureGuidePresentation::Suspended, false) => {
                self.presentation = FeatureGuidePresentation::Visible;
            }
            _ => {}
        }
    }

    pub(crate) fn open_manually(&mut self) {
        self.presentation = FeatureGuidePresentation::Visible;
        if self.status == FeatureGuideStatus::NotSeen {
            self.status = FeatureGuideStatus::InProgress;
        }
    }

    pub(crate) fn close_for_session(&mut self) {
        self.presentation = FeatureGuidePresentation::ClosedForSession;
    }

    pub(crate) fn suppress(&mut self) {
        self.status = FeatureGuideStatus::Suppressed;
        self.presentation = FeatureGuidePresentation::ClosedForSession;
        self.auto_eligible = false;
    }

    pub(crate) fn complete(&mut self) {
        self.status = FeatureGuideStatus::Completed;
        self.presentation = FeatureGuidePresentation::ClosedForSession;
        self.auto_eligible = false;
    }

    pub(crate) fn select_topic(&mut self, topic: FeatureGuideTopic) {
        self.topic = topic;
    }

    pub(crate) fn toggle_orchestration_details(&mut self) {
        self.orchestration_details_expanded = !self.orchestration_details_expanded;
    }

    pub(crate) fn orchestration_details_expanded(self) -> bool {
        self.orchestration_details_expanded
    }

    pub(crate) fn begin_settings_transition(
        &mut self,
        transition: FeatureGuideSettingsTransition,
    ) -> u64 {
        self.settings_transition_generation = self.settings_transition_generation.saturating_add(1);
        self.settings_transition = transition;
        self.settings_transition_generation
    }

    pub(crate) fn finish_settings_transition(&mut self, generation: u64) -> bool {
        if generation != self.settings_transition_generation {
            return false;
        }
        self.settings_transition = FeatureGuideSettingsTransition::None;
        true
    }

    pub(crate) fn settings_transition(self) -> FeatureGuideSettingsTransition {
        self.settings_transition
    }

    pub(crate) fn settings_transition_generation(self) -> u64 {
        self.settings_transition_generation
    }

    pub(crate) fn should_render(self) -> bool {
        self.is_visible() || self.settings_transition != FeatureGuideSettingsTransition::None
    }

    pub(crate) fn topic_position(self) -> usize {
        FeatureGuideTopic::ALL
            .iter()
            .position(|topic| *topic == self.topic)
            .unwrap_or(0)
            + 1
    }

    pub(crate) fn next_topic(&mut self) {
        let index = FeatureGuideTopic::ALL
            .iter()
            .position(|topic| *topic == self.topic)
            .unwrap_or(0);
        self.topic = FeatureGuideTopic::ALL[(index + 1).min(FeatureGuideTopic::ALL.len() - 1)];
    }

    pub(crate) fn previous_topic(&mut self) {
        let index = FeatureGuideTopic::ALL
            .iter()
            .position(|topic| *topic == self.topic)
            .unwrap_or(0);
        self.topic = FeatureGuideTopic::ALL[index.saturating_sub(1)];
    }

    pub(crate) fn is_pending(self) -> bool {
        self.presentation == FeatureGuidePresentation::Pending
    }

    pub(crate) fn is_visible(self) -> bool {
        self.presentation == FeatureGuidePresentation::Visible
    }

    pub(crate) fn serialized_status(self) -> &'static str {
        match self.status {
            FeatureGuideStatus::NotSeen => "not_seen",
            FeatureGuideStatus::InProgress => "in_progress",
            FeatureGuideStatus::Completed => "completed",
            FeatureGuideStatus::Suppressed => "suppressed",
        }
    }

    pub(crate) fn serialized_topic(self) -> &'static str {
        match self.topic {
            FeatureGuideTopic::AgentOrchestration => "agent_orchestration",
            FeatureGuideTopic::VoiceGroupControl => "voice_group_control",
            FeatureGuideTopic::AgentCollaboration => "agent_collaboration",
            FeatureGuideTopic::LiveNodeShortcuts => "live_node_shortcuts",
            FeatureGuideTopic::WorkspaceLayouts => "workspace_layouts",
            FeatureGuideTopic::AgentNotifications => "agent_notifications",
        }
    }
}
