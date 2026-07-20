use super::{VoiceSettingsState, VoiceSetupDestination};
use crate::settings::voice::{
    evaluate_voice_readiness, AsrModelInstallState, TtsModelInstallState, VoiceEntryState,
    VoiceReadinessInputs, VoiceRequirement, VoiceRequirementState,
};

impl VoiceSettingsState {
    pub(crate) fn readiness(&self) -> VoiceEntryState {
        #[cfg(feature = "e2e-test-support")]
        if let Some(readiness) = self.readiness_override {
            return readiness;
        }
        evaluate_voice_readiness(VoiceReadinessInputs {
            microphone: self.microphone_requirement,
            speech_recognition: self.selected_asr_requirement(),
            agent_provider: self.provider_requirement,
            spoken_replies: self.selected_tts_requirement(),
            model_install_progress: self.active_model_install_progress(),
            setup_was_completed: self.setup_was_completed,
        })
    }

    pub(crate) fn set_microphone_requirement(&mut self, state: VoiceRequirementState) {
        self.microphone_requirement = state;
    }

    #[cfg(test)]
    pub(crate) fn set_provider_requirement(&mut self, state: VoiceRequirementState) {
        self.provider_requirement = state;
    }

    pub(crate) fn invalidate_provider_requirement(&mut self) {
        self.provider_requirement = VoiceRequirementState::Missing;
        self.reset_provider_discovery();
    }

    pub(crate) fn requirement_state(&self, requirement: VoiceRequirement) -> VoiceRequirementState {
        match requirement {
            VoiceRequirement::Microphone => self.microphone_requirement,
            VoiceRequirement::SpeechRecognition => self.selected_asr_requirement(),
            VoiceRequirement::AgentProvider => self.provider_requirement,
            VoiceRequirement::SpokenReplies => self.selected_tts_requirement(),
        }
    }

    pub(crate) fn reconcile_setup_completion(&mut self) -> bool {
        if self.setup_was_completed || self.readiness() != VoiceEntryState::Ready {
            return false;
        }
        self.setup_was_completed = true;
        self.setup_destination = VoiceSetupDestination::Overview;
        self.saved.setup_completed = Some(true);
        self.persist_string(super::KEY_VOICE_SETUP_COMPLETED, "true");
        true
    }

    fn selected_asr_requirement(&self) -> VoiceRequirementState {
        if self.values.asr_provider() == asr::AsrProviderId::CloudAzure {
            return if self.values.azure_asr_profile().is_ready()
                && asr::validate_silero_vad_installation(&self.model_root).is_ok()
            {
                VoiceRequirementState::Ready
            } else {
                VoiceRequirementState::Missing
            };
        }
        let Some(model_id) = self.values.asr_model_id() else {
            return VoiceRequirementState::Missing;
        };
        match self.model_manager.installation(model_id) {
            Some(AsrModelInstallState::Ready) => VoiceRequirementState::Ready,
            Some(AsrModelInstallState::Installing { downloaded, total }) => {
                VoiceRequirementState::Installing {
                    completed: *downloaded,
                    total: *total,
                }
            }
            Some(AsrModelInstallState::Failed(_)) => VoiceRequirementState::Failed,
            Some(AsrModelInstallState::NotInstalled) | None => VoiceRequirementState::Missing,
        }
    }

    fn selected_tts_requirement(&self) -> VoiceRequirementState {
        let Some(provider_id) = self.values.active_tts_provider_id() else {
            return VoiceRequirementState::Missing;
        };
        if provider_id.as_str() == tts::MINIMAX_TTS_PROVIDER_ID {
            return match self.minimax_readiness() {
                tts::RemoteTtsReadiness::Ready => VoiceRequirementState::Ready,
                tts::RemoteTtsReadiness::Checking => VoiceRequirementState::Installing {
                    completed: 0,
                    total: 1,
                },
                tts::RemoteTtsReadiness::TemporarilyUnavailable
                | tts::RemoteTtsReadiness::InvalidCredential
                | tts::RemoteTtsReadiness::VoiceUnavailable => VoiceRequirementState::Failed,
                tts::RemoteTtsReadiness::Unconfigured
                | tts::RemoteTtsReadiness::Configured
                | tts::RemoteTtsReadiness::VoiceValidationRequired => {
                    VoiceRequirementState::Missing
                }
            };
        }
        if provider_id.as_str() != tts::LOCAL_TTS_PROVIDER_ID {
            return VoiceRequirementState::Failed;
        }
        let (Some(model_id), Some(voice_id)) =
            (self.values.tts_model_id(), self.values.tts_voice_id())
        else {
            return VoiceRequirementState::Missing;
        };
        let voice_exists = tts::builtin_model_catalog()
            .find(model_id)
            .and_then(|model| model.voice(voice_id))
            .is_some();
        if !voice_exists {
            return VoiceRequirementState::Failed;
        }
        match self.tts_model_manager.installation(model_id) {
            Some(TtsModelInstallState::Ready) => VoiceRequirementState::Ready,
            Some(TtsModelInstallState::Installing { downloaded, total })
            | Some(TtsModelInstallState::Cancelling { downloaded, total }) => {
                VoiceRequirementState::Installing {
                    completed: *downloaded,
                    total: *total,
                }
            }
            Some(TtsModelInstallState::Failed(_)) => VoiceRequirementState::Failed,
            Some(TtsModelInstallState::NotInstalled) | None => VoiceRequirementState::Missing,
        }
    }

    pub(crate) fn active_model_install_progress(&self) -> Option<(u64, u64)> {
        let mut completed = 0_u64;
        let mut total = 0_u64;
        for entry in self.model_manager.entries() {
            if let AsrModelInstallState::Installing {
                downloaded,
                total: model_total,
            } = entry.installation()
            {
                completed = completed.saturating_add(*downloaded);
                total = total.saturating_add(*model_total);
            }
        }
        for entry in self.tts_model_manager.entries() {
            if let TtsModelInstallState::Installing {
                downloaded,
                total: model_total,
            }
            | TtsModelInstallState::Cancelling {
                downloaded,
                total: model_total,
            } = entry.installation()
            {
                completed = completed.saturating_add(*downloaded);
                total = total.saturating_add(*model_total);
            }
        }
        (total > 0).then_some((completed, total))
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/voice_settings_readiness_tests.rs"]
mod voice_settings_readiness_tests;
