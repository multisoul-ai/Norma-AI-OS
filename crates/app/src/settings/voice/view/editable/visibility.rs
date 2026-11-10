use super::VoiceSettingsInputs;
use gpui::App;

impl VoiceSettingsInputs {
    /// Saved keys have no reveal control, so this stays false after the editor syncs.
    pub(crate) fn api_key_revealed(&self, cx: &App) -> bool {
        self.api_key.read(cx).revealed
    }
}
