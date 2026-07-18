use super::{AsrModelManagerState, TtsModelManagerState, VoiceSettingsState};
use std::path::PathBuf;

impl VoiceSettingsState {
    pub(crate) fn assume_legacy_ready_for_e2e(&mut self) {
        self.readiness_override = Some(crate::settings::voice::VoiceEntryState::Ready);
        self.advanced_expanded = true;
    }

    pub(crate) fn assume_visual_preview_ready_for_e2e(&mut self) {
        self.readiness_override = Some(crate::settings::voice::VoiceEntryState::Ready);
        self.reviewing_setup = false;
        self.advanced_expanded = false;
    }

    pub(crate) fn reset_voice_setup_for_e2e(&mut self) {
        self.readiness_override = None;
        self.advanced_expanded = false;
        self.microphone_requirement = crate::settings::voice::VoiceRequirementState::Missing;
        self.provider_requirement = crate::settings::voice::VoiceRequirementState::Missing;
        self.setup_was_completed = false;
        self.reviewing_setup = false;
    }

    pub(crate) fn set_setup_requirements_for_e2e(
        &mut self,
        microphone_ready: bool,
        provider_ready: bool,
    ) {
        use crate::settings::voice::VoiceRequirementState;
        self.microphone_requirement = if microphone_ready {
            VoiceRequirementState::Ready
        } else {
            VoiceRequirementState::Missing
        };
        self.provider_requirement = if provider_ready {
            VoiceRequirementState::Ready
        } else {
            VoiceRequirementState::Missing
        };
    }

    pub(crate) fn reset_model_root_for_e2e(&mut self, model_root: PathBuf) {
        self.model_root = model_root;
        self.model_manager = AsrModelManagerState::unscanned(&asr::builtin_model_catalog());
        self.tts_model_manager = TtsModelManagerState::unscanned(&tts::builtin_model_catalog());
    }
}
