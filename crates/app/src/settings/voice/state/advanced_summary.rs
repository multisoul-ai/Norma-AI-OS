use super::VoiceSettingsState;
use crate::settings::voice::{VoiceRequirement, VoiceRequirementState};

/// 「更多设置」里的可折叠分组，手风琴式展开。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdvancedGroup {
    Model,
    Recognition,
    Jev,
    Reply,
    Diagnostics,
}

/// 分组标题右侧的状态标签；只描述就绪度，文案与颜色在视图层映射。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdvancedStatus {
    Unconfigured,
    Pending,
    NeedsConnection,
    Ready,
}

/// 分组标题下的一行摘要，携带结构化参数，由视图层走 i18n。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AdvancedSummary {
    Model(Option<String>),
    Source(asr::AsrProviderId),
    Reply { announcements: bool, glow: bool },
    Default,
}

impl VoiceSettingsState {
    pub(crate) fn advanced_group_status(&self, group: AdvancedGroup) -> Option<AdvancedStatus> {
        match group {
            AdvancedGroup::Model => Some(self.model_status()),
            AdvancedGroup::Recognition => Some(self.recognition_status()),
            // Jev 状态活在其面板实体里，这里不伪造；Reply/Diagnostics 无就绪语义。
            AdvancedGroup::Jev | AdvancedGroup::Reply | AdvancedGroup::Diagnostics => None,
        }
    }

    pub(crate) fn advanced_group_summary(&self, group: AdvancedGroup) -> AdvancedSummary {
        match group {
            AdvancedGroup::Model => AdvancedSummary::Model(self.configured_model()),
            AdvancedGroup::Recognition => AdvancedSummary::Source(self.values.asr_provider()),
            AdvancedGroup::Reply => AdvancedSummary::Reply {
                announcements: self.values.agent_task_announcements_enabled(),
                glow: self.values.voice_window_inset_enabled(),
            },
            AdvancedGroup::Jev | AdvancedGroup::Diagnostics => AdvancedSummary::Default,
        }
    }

    fn configured_model(&self) -> Option<String> {
        let model = self.values.active_provider_profile().model().trim();
        (!model.is_empty()).then(|| model.to_string())
    }

    fn model_status(&self) -> AdvancedStatus {
        if self.provider_requirement == VoiceRequirementState::Ready {
            AdvancedStatus::Ready
        } else if self.configured_model().is_none() {
            AdvancedStatus::Unconfigured
        } else {
            AdvancedStatus::NeedsConnection
        }
    }

    fn recognition_status(&self) -> AdvancedStatus {
        if self.requirement_state(VoiceRequirement::SpeechRecognition)
            == VoiceRequirementState::Ready
        {
            AdvancedStatus::Ready
        } else if self.values.asr_provider() == asr::AsrProviderId::CloudAzure
            && self.values.azure_asr_profile().credential_ref().is_none()
        {
            AdvancedStatus::Unconfigured
        } else {
            AdvancedStatus::Pending
        }
    }
}
