use super::values::{KEY_DESKTOP_MODE, KEY_INTERACTION_MODE};
use super::{DesktopVoiceMode, VoiceInteractionMode, VoiceSettingsState};

impl VoiceSettingsState {
    pub(crate) fn select_interaction_mode(&mut self, mode: VoiceInteractionMode) {
        self.invalidate_a1_preparation();
        self.saved.interaction_mode = Some(mode);
        self.persist_string(KEY_INTERACTION_MODE, mode.as_str());
        self.refresh_values();
    }

    pub(crate) fn select_desktop_mode(&mut self, mode: DesktopVoiceMode) {
        self.invalidate_a1_preparation();
        if self.values.desktop_mode() != mode {
            self.status = None;
            #[cfg(feature = "e2e-test-support")]
            {
                self.readiness_override = None;
            }
        }
        self.saved.desktop_mode = Some(mode);
        self.persist_string(KEY_DESKTOP_MODE, mode.as_str());
        self.refresh_values();
    }
}
