use crate::TtsVoiceId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtsVoiceOrigin {
    System,
    Cloned,
    Generated,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ProviderVoiceDescriptor {
    id: TtsVoiceId,
    display_name: String,
    origin: TtsVoiceOrigin,
}

impl ProviderVoiceDescriptor {
    pub fn new(id: TtsVoiceId, display_name: impl Into<String>, origin: TtsVoiceOrigin) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            origin,
        }
    }

    pub fn id(&self) -> &TtsVoiceId {
        &self.id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn origin(&self) -> TtsVoiceOrigin {
        self.origin
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub struct ProviderVoiceCatalog {
    voices: Vec<ProviderVoiceDescriptor>,
}

impl ProviderVoiceCatalog {
    pub fn new(voices: Vec<ProviderVoiceDescriptor>) -> Self {
        Self { voices }
    }

    pub fn voices(&self) -> &[ProviderVoiceDescriptor] {
        &self.voices
    }

    pub fn contains(&self, voice_id: &TtsVoiceId) -> bool {
        self.voices.iter().any(|voice| voice.id() == voice_id)
    }
}
