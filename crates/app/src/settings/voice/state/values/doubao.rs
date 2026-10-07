use super::{DesktopVoiceMode, VoiceInteractionMode, VoiceSettingsValues};
use tts::providers::doubao::{default_voice, find_voice, DoubaoVoice};

pub(super) fn resolve(saved: Option<&str>) -> &'static DoubaoVoice {
    saved.and_then(find_voice).unwrap_or_else(default_voice)
}

impl VoiceSettingsValues {
    pub(crate) fn desktop_mode(&self) -> DesktopVoiceMode {
        self.desktop_mode
    }

    pub(crate) fn doubao_credential_ref(&self) -> Option<&str> {
        self.doubao_credential_ref.as_deref()
    }

    pub(crate) fn doubao_interaction_mode(&self) -> VoiceInteractionMode {
        self.doubao_interaction_mode
    }

    pub(crate) fn doubao_voice(&self) -> &'static DoubaoVoice {
        self.doubao_voice
    }
}
