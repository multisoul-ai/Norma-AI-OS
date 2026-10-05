use super::{values::KEY_DOUBAO_SPEAKER, VoiceSettingsState};
use tts::providers::doubao::{find_voice, voices, DoubaoVoice};

impl VoiceSettingsState {
    pub(crate) fn select_doubao_voice(&mut self, id: &str) -> bool {
        let Some(voice) = find_voice(id) else {
            return false;
        };
        self.saved.doubao_speaker = Some(voice.id.clone());
        self.persist_string(KEY_DOUBAO_SPEAKER, &voice.id);
        self.refresh_values();
        true
    }

    #[cfg(test)]
    pub(crate) fn filtered_doubao_voices(&self) -> Vec<&'static DoubaoVoice> {
        Self::doubao_voices_matching(&self.doubao_voice_search)
    }

    pub(crate) fn doubao_voices_matching(query: &str) -> Vec<&'static DoubaoVoice> {
        let query = query.trim().to_lowercase();
        voices()
            .iter()
            .filter(|voice| {
                query.is_empty()
                    || voice.name.to_lowercase().contains(&query)
                    || voice.id.to_lowercase().contains(&query)
                    || voice.language.to_lowercase().contains(&query)
            })
            .collect()
    }
}
