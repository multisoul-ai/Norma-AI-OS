use super::{
    AgentProviderKind, AsrModelManagerState, TtsModelManagerState, VoiceSettingsState,
    VoiceSetupDestination,
};
use std::path::PathBuf;

impl VoiceSettingsState {
    pub(crate) fn configure_doubao_for_e2e(&mut self, secret: &str) {
        let store = self
            .storage
            .as_ref()
            .expect("E2E voice settings need a SQLite store");
        crate::settings::voice::credentials::store(store, "doubao-desktop", secret)
            .expect("E2E SQLite should accept synthetic credential");
        self.saved.doubao_credential_ref = Some("doubao-desktop".into());
        self.microphone_requirement = crate::settings::voice::VoiceRequirementState::Ready;
        self.readiness_override = None;
        self.refresh_values();
    }
    pub(crate) fn assume_legacy_ready_for_e2e(&mut self) {
        self.readiness_override = Some(crate::settings::voice::VoiceEntryState::Ready);
        self.setup_destination = VoiceSetupDestination::Overview;
        self.advanced_expanded = true;
    }

    pub(crate) fn assume_agent_provider_ready_for_e2e(&mut self) {
        self.saved.agent_provider = Some(AgentProviderKind::Ollama);
        self.refresh_values();
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

    pub(crate) fn assume_visual_preview_azure_asr_ready_for_e2e(&mut self) {
        self.readiness_override = Some(crate::settings::voice::VoiceEntryState::Ready);
        self.reviewing_setup = true;
        self.setup_destination = VoiceSetupDestination::Overview;
        self.advanced_expanded = true;
        self.saved.azure_asr_region = Some("eastus".to_string());
        self.saved.azure_asr_locale = Some("zh-CN".to_string());
        self.saved.azure_asr_credential_ref = Some("visual-preview".to_string());
        self.saved.azure_asr_ready = Some(true);
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
        self.saved.asr_model_id = None;
        self.saved.tts_model_id = None;
        self.saved.tts_voice_id = None;
        self.saved.active_tts_provider_id = None;
        self.tts_picker = None;
        self.refresh_values();
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

    pub(crate) fn assume_minimax_connected_for_e2e(&mut self) {
        let model_id = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
            .expect("built-in MiniMax Turbo model ID must remain valid");
        let system_voice =
            tts::TtsVoiceId::parse("system_voice_e2e").expect("e2e System Voice ID must be valid");
        let cloned_voice =
            tts::TtsVoiceId::parse("cloned_voice_e2e").expect("e2e Cloned Voice ID must be valid");
        let generated_voice = tts::TtsVoiceId::parse("generated_voice_e2e")
            .expect("e2e Generated Voice ID must be valid");
        self.saved.minimax_tts_credential_ref = Some("e2e-minimax-account".to_string());
        self.saved.minimax_tts_api_key = Some("synthetic-e2e-minimax-key".to_string());
        self.saved.minimax_tts_model_id = Some(model_id);
        self.saved.minimax_tts_voice_id = Some(system_voice.clone());
        self.saved.minimax_tts_voice_provenance = Some(tts::VoiceSelectionProvenance::Catalog);
        self.minimax_account_status = tts::RemoteAccountStatus::Ready;
        self.minimax_voice_catalog = tts::ProviderVoiceCatalog::new(vec![
            tts::ProviderVoiceDescriptor::new(
                system_voice,
                "System Narrator",
                tts::TtsVoiceOrigin::System,
            ),
            tts::ProviderVoiceDescriptor::new(
                cloned_voice,
                "Cloned Narrator",
                tts::TtsVoiceOrigin::Cloned,
            ),
            tts::ProviderVoiceDescriptor::new(
                generated_voice,
                "Generated Narrator",
                tts::TtsVoiceOrigin::Generated,
            ),
        ]);
        self.refresh_values();
    }

    pub(crate) fn assume_minimax_temporary_network_failure_for_e2e(&mut self) {
        self.readiness_override = None;
        self.reviewing_setup = false;
        self.setup_was_completed = true;
        self.saved.setup_completed = Some(true);
        self.microphone_requirement = crate::settings::voice::VoiceRequirementState::Ready;
        self.provider_requirement = crate::settings::voice::VoiceRequirementState::Ready;
        let asr_id = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
            .expect("built-in Zipformer Bilingual model ID must remain valid");
        self.saved.asr_model_id = Some(asr_id.clone());
        self.model_manager.mark_ready(&asr_id);
        self.assume_minimax_connected_for_e2e();
        self.saved.active_tts_provider_id = Some(
            tts::TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
                .expect("built-in MiniMax Provider ID must remain valid"),
        );
        self.minimax_account_status = tts::RemoteAccountStatus::TemporarilyUnavailable;
        self.status = Some(super::error_status(
            "MiniMax is temporarily unavailable; configuration was preserved.",
        ));
        self.refresh_values();
    }

    pub(crate) fn assume_minimax_connected_with_system_voices_for_e2e(&mut self, count: usize) {
        let model_id = tts::TtsModelId::parse(tts::MINIMAX_SPEECH_TURBO_MODEL_ID)
            .expect("built-in MiniMax Turbo model ID must remain valid");
        let voices = (0..count)
            .map(|index| {
                let voice_id = tts::TtsVoiceId::parse(format!("system_voice_{index:03}"))
                    .expect("generated e2e System Voice ID must be valid");
                tts::ProviderVoiceDescriptor::new(
                    voice_id,
                    format!("System Voice {index:03}"),
                    tts::TtsVoiceOrigin::System,
                )
            })
            .collect::<Vec<_>>();
        self.saved.minimax_tts_credential_ref = Some("e2e-minimax-account".to_string());
        self.saved.minimax_tts_api_key = Some("synthetic-e2e-minimax-key".to_string());
        self.saved.minimax_tts_model_id = Some(model_id);
        self.saved.minimax_tts_voice_id = voices.first().map(|voice| voice.id().clone());
        self.saved.minimax_tts_voice_provenance = Some(tts::VoiceSelectionProvenance::Catalog);
        self.minimax_account_status = tts::RemoteAccountStatus::Ready;
        self.minimax_voice_catalog = tts::ProviderVoiceCatalog::new(voices);
        self.refresh_values();
    }
}
