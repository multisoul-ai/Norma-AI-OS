use super::VoiceSettingsState;

impl VoiceSettingsState {
    pub(crate) fn set_system_voice_names(&mut self, names: Vec<String>) {
        self.system_voice_names = Some(names);
    }

    pub(crate) fn system_voice_names(&self) -> &[String] {
        self.system_voice_names.as_deref().unwrap_or(&[])
    }

    pub(crate) fn system_voice_names_loaded(&self) -> bool {
        self.system_voice_names.is_some()
    }

    pub(crate) fn begin_system_voice_scan(&mut self) -> u64 {
        self.system_voice_scan_generation = self.system_voice_scan_generation.wrapping_add(1);
        self.system_voice_scan_generation
    }

    pub(crate) fn system_voice_scan_generation(&self) -> u64 {
        self.system_voice_scan_generation
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/voice_settings_tts_output_mode_tests.rs"]
mod voice_settings_tts_output_mode_tests;
