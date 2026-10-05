use super::VoiceSettingsValues;
use tts::providers::doubao::{default_voice, find_voice, DoubaoVoice};

pub(super) fn resolve(saved: Option<&str>) -> &'static DoubaoVoice {
    saved.and_then(find_voice).unwrap_or_else(default_voice)
}

impl VoiceSettingsValues {
    pub(crate) fn doubao_voice(&self) -> &'static DoubaoVoice {
        self.doubao_voice
    }
}
