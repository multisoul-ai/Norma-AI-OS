use super::{values::KEY_ASR_PROVIDER, VoiceSettingsState};
use crate::settings::voice::VoiceRequirementState;

impl VoiceSettingsState {
    pub(crate) fn invalidate_a1_preparation(&mut self) {
        self.a1_preparation_cancellation.cancel();
        self.a1_validation_generation = self.a1_validation_generation.wrapping_add(1);
        if matches!(
            self.a1_requirement,
            VoiceRequirementState::Installing { .. }
        ) {
            self.a1_requirement = VoiceRequirementState::Missing;
        }
    }
    pub(crate) fn a1_requirement(&self) -> VoiceRequirementState {
        self.a1_requirement
    }
    pub(crate) fn a1_validation_error(&self) -> Option<&str> {
        self.a1_validation_error.as_deref()
    }
    pub(crate) fn a1_preparation_cancellation(&self) -> asr::RecognitionCancellation {
        self.a1_preparation_cancellation.clone()
    }
    pub(crate) fn begin_a1_validation(&mut self) -> u64 {
        self.a1_preparation_cancellation.cancel();
        self.a1_preparation_cancellation = asr::RecognitionCancellation::new();
        self.a1_validation_generation = self.a1_validation_generation.wrapping_add(1);
        self.a1_requirement = VoiceRequirementState::Installing {
            completed: 0,
            total: 1,
        };
        self.a1_validation_error = None;
        self.a1_validation_generation
    }
    pub(crate) fn finish_a1_validation(
        &mut self,
        generation: u64,
        result: Result<(), String>,
    ) -> bool {
        if generation != self.a1_validation_generation {
            return false;
        }
        self.a1_requirement = if result.is_ok() {
            VoiceRequirementState::Ready
        } else {
            VoiceRequirementState::Failed
        };
        self.a1_validation_error = result.err();
        true
    }
    pub(crate) fn activate_local_a1(&mut self) -> bool {
        if self.a1_requirement != VoiceRequirementState::Ready
            || !self
                .values
                .desktop_mode()
                .supports_local_a1(self.values.interaction_mode())
        {
            return false;
        }
        self.saved.asr_provider = Some(asr::AsrProviderId::LocalA1);
        self.persist_string(KEY_ASR_PROVIDER, asr::AsrProviderId::LocalA1.as_str());
        self.refresh_values();
        true
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/voice_a1_settings_tests.rs"]
mod tests;
