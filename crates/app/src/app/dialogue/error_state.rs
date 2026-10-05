//! Dialogue startup-error state and the associated microphone recovery affordance.

use super::{error::start_error_label, DialogueControlState, DialogueStartError};
use voice_sherpa::SpeechError;

impl DialogueControlState {
    pub(crate) fn microphone_settings_recovery_available(&self) -> bool {
        self.microphone_settings_recovery_available
    }

    pub(crate) fn dismiss_error(&mut self) {
        self.error_message = None;
        self.microphone_settings_recovery_available = false;
    }

    pub(crate) fn mark_start_error(&mut self, error: DialogueStartError) {
        self.record_start_error(&error);
    }

    pub(crate) fn mark_runtime_error(&mut self, message: impl Into<String>) {
        self.stop_pcm_stream();
        self.session.on_listening_stopped();
        self.microphone_settings_recovery_available = false;
        self.error_message = Some(ui::i18n::message(
            "chrome.speech_recognition_failed",
            &[("arg0", (message.into()).to_string())],
        ));
    }

    pub(super) fn record_start_error(&mut self, error: &DialogueStartError) {
        self.stop_pcm_stream();
        self.session.on_listening_stopped();
        self.microphone_settings_recovery_available = matches!(
            error,
            DialogueStartError::Speech(SpeechError::MicrophoneDenied)
        );
        self.error_message = Some(start_error_label(error));
    }
}
