use super::values::{KEY_AGENT_TASK_ANNOUNCEMENTS_ENABLED, KEY_VOICE_WINDOW_INSET_ENABLED};
use super::VoiceSettingsState;

impl VoiceSettingsState {
    pub(crate) fn toggle_agent_task_announcements(&mut self) {
        let next = !self.values.agent_task_announcements_enabled;
        self.saved.agent_task_announcements_enabled = Some(next);
        self.persist_string(
            KEY_AGENT_TASK_ANNOUNCEMENTS_ENABLED,
            if next { "true" } else { "false" },
        );
        self.refresh_values();
    }

    pub(crate) fn toggle_voice_window_inset(&mut self) {
        let next = !self.values.voice_window_inset_enabled;
        self.saved.voice_window_inset_enabled = Some(next);
        self.persist_string(
            KEY_VOICE_WINDOW_INSET_ENABLED,
            if next { "true" } else { "false" },
        );
        self.refresh_values();
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/voice_settings_announcements_tests.rs"]
mod tests;
