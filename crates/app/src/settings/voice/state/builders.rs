use super::VoiceSettingsState;
#[cfg(any(test, feature = "e2e-test-support"))]
use super::{EnvVoiceSettings, SavedVoiceSettings};

impl VoiceSettingsState {
    #[cfg(any(test, feature = "e2e-test-support"))]
    pub(crate) fn in_memory_default() -> Self {
        Self::from_sources(SavedVoiceSettings::default(), EnvVoiceSettings::default())
    }
}
