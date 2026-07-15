use super::{EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState};

impl VoiceSettingsState {
    pub(crate) fn in_memory_default() -> Self {
        Self::from_sources(SavedVoiceSettings::default(), EnvVoiceSettings::default())
    }
}
