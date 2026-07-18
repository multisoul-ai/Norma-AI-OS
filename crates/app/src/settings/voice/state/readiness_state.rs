use super::VoiceSettingsState;
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
            model_install_progress: self.selected_model_install_progress(),
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
        self.setup_models_expanded = false;
        self.saved.setup_completed = Some(true);
        self.persist_string(super::KEY_VOICE_SETUP_COMPLETED, "true");
        true
    }

    fn selected_asr_requirement(&self) -> VoiceRequirementState {
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

    pub(crate) fn selected_model_install_progress(&self) -> Option<(u64, u64)> {
        let asr_state = self.selected_asr_requirement();
        let tts_state = self.selected_tts_requirement();
        if !matches!(asr_state, VoiceRequirementState::Installing { .. })
            && !matches!(tts_state, VoiceRequirementState::Installing { .. })
        {
            return None;
        }
        let asr_catalog = asr::builtin_model_catalog();
        let tts_catalog = tts::builtin_model_catalog();
        let asr_total = self
            .values
            .asr_model_id()
            .and_then(|id| asr_catalog.find(id))
            .map(|model| model.artifact().archive_bytes())
            .unwrap_or(0);
        let tts_total = self
            .values
            .tts_model_id()
            .and_then(|id| tts_catalog.find(id))
            .map(|model| model.package().download_bytes())
            .unwrap_or(0);
        let completed = install_progress_bytes(asr_state, asr_total)
            .saturating_add(install_progress_bytes(tts_state, tts_total));
        Some((completed, asr_total.saturating_add(tts_total)))
    }
}

fn install_progress_bytes(state: VoiceRequirementState, expected_total: u64) -> u64 {
    match state {
        VoiceRequirementState::Ready => expected_total,
        VoiceRequirementState::Installing { completed, .. } => completed.min(expected_total),
        VoiceRequirementState::Missing | VoiceRequirementState::Failed => 0,
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/voice_settings_readiness_tests.rs"]
mod voice_settings_readiness_tests;
