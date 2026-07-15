use serde::{Deserialize, Serialize};

/// Stable identity for a pinned Local TTS model package.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct TtsModelId(String);

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TtsModelIdError {
    #[error("TTS model id cannot be blank")]
    Blank,
}

impl TtsModelId {
    pub fn parse(value: impl Into<String>) -> Result<Self, TtsModelIdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(TtsModelIdError::Blank);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable identity for a voice exposed by one curated TTS model.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct TtsVoiceId(String);

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TtsVoiceIdError {
    #[error("TTS voice id cannot be blank")]
    Blank,
}

impl TtsVoiceId {
    pub fn parse(value: impl Into<String>) -> Result<Self, TtsVoiceIdError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(TtsVoiceIdError::Blank);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
