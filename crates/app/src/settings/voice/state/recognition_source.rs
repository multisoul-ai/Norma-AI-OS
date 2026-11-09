use super::{
    values::{KEY_ASR_PROVIDER, KEY_LOCAL_ASR_PROVIDER},
    VoiceSettingsState,
};

impl VoiceSettingsState {
    pub(crate) fn preferred_local_provider(&self) -> asr::AsrProviderId {
        self.saved
            .asr_provider
            .filter(|id| *id != asr::AsrProviderId::CloudAzure)
            .or(self
                .saved
                .local_asr_provider
                .filter(|id| *id != asr::AsrProviderId::CloudAzure))
            .unwrap_or(asr::AsrProviderId::LocalSherpa)
    }

    pub(super) fn remember_local_provider(&mut self, provider: asr::AsrProviderId) {
        if provider != asr::AsrProviderId::CloudAzure {
            self.saved.local_asr_provider = Some(provider);
            self.persist_string(KEY_LOCAL_ASR_PROVIDER, provider.as_str());
        }
    }

    pub(crate) fn activate_local_recognition(&mut self) -> bool {
        match self.preferred_local_provider() {
            asr::AsrProviderId::LocalA1
                if self.values.desktop_mode() == super::DesktopVoiceMode::Legacy
                    && self.values.interaction_mode()
                        == super::VoiceInteractionMode::ContinuousListening =>
            {
                self.invalidate_a1_preparation();
                self.saved.asr_provider = Some(asr::AsrProviderId::LocalA1);
                self.remember_local_provider(asr::AsrProviderId::LocalA1);
                self.persist_string(KEY_ASR_PROVIDER, asr::AsrProviderId::LocalA1.as_str());
                self.refresh_values();
                true
            }
            asr::AsrProviderId::LocalA1 => self.activate_local_a1(),
            asr::AsrProviderId::LocalSherpa => {
                self.activate_local_sherpa_asr();
                true
            }
            asr::AsrProviderId::CloudAzure => false,
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/voice_recognition_source_tests.rs"]
mod tests;
