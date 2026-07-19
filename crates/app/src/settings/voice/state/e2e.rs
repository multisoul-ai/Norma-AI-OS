use super::{
    AgentProviderKind, AsrModelManagerState, TtsModelManagerState, VoiceSettingsState,
    VoiceSetupDestination,
};
use std::path::PathBuf;

impl VoiceSettingsState {
    pub(crate) fn assume_legacy_ready_for_e2e(&mut self) {
        self.readiness_override = Some(crate::settings::voice::VoiceEntryState::Ready);
        self.setup_destination = VoiceSetupDestination::Overview;
        self.advanced_expanded = true;
    }

    pub(crate) fn assume_visual_preview_ready_for_e2e(&mut self) {
        self.readiness_override = Some(crate::settings::voice::VoiceEntryState::Ready);
        self.reviewing_setup = false;
        self.setup_destination = VoiceSetupDestination::Overview;
        self.advanced_expanded = false;
    }

    pub(crate) fn assume_visual_preview_api_key_for_e2e(&mut self) {
        self.readiness_override = Some(crate::settings::voice::VoiceEntryState::Ready);
        self.reviewing_setup = true;
        self.setup_destination = VoiceSetupDestination::Overview;
        self.advanced_expanded = true;
        self.saved.agent_provider = Some(AgentProviderKind::OpenAiCompatible);
        self.saved.openai_profile.api_key = Some("synthetic-preview-key".to_string());
        self.saved.openai_profile.credential_ref = Some("visual-preview".to_string());
        self.refresh_values();
    }

    pub(crate) fn assume_visual_preview_zipvoice_reference_for_e2e(&mut self) {
        self.readiness_override = Some(crate::settings::voice::VoiceEntryState::Ready);
        self.reviewing_setup = true;
        self.setup_destination = VoiceSetupDestination::SpokenReplies;
        self.advanced_expanded = false;
        let id = tts::TtsModelId::parse(tts::ZIPVOICE_DISTILL_INT8_MODEL_ID)
            .expect("built-in ZipVoice model ID must remain valid");
        self.tts_model_manager.mark_ready(&id);
    }

    pub(crate) fn reset_voice_setup_for_e2e(&mut self) {
        self.readiness_override = None;
        self.setup_destination = VoiceSetupDestination::Overview;
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
        self.zipvoice_reference = super::ZipVoiceReferenceSettings::default();
    }
}
