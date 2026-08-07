//! Speech-error recovery state for the one-shot voice controller.

use super::VoiceControlState;
use voice_sherpa::SpeechError;

impl VoiceControlState {
    pub(crate) fn dismiss_error(&mut self) {
        if matches!(self.phase(), super::VoicePhase::Error { .. }) {
            self.phase = super::VoicePhase::Idle;
        }
        self.microphone_settings_recovery_available = false;
    }

    pub(crate) fn set_speech_error(&mut self, error: &SpeechError) {
        self.set_error(format!("Speech recognition failed: {error}"));
        self.microphone_settings_recovery_available =
            super::super::microphone_permission_recovery_url(error).is_some();
    }

    pub(crate) fn microphone_settings_recovery_available(&self) -> bool {
        self.microphone_settings_recovery_available
    }
}
