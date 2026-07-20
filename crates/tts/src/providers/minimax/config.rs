use crate::{
    TtsError, TtsErrorKind, TtsModelId, TtsVoiceId, MINIMAX_SPEECH_HD_MODEL_ID,
    MINIMAX_SPEECH_TURBO_MODEL_ID,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MiniMaxTtsConfig {
    model_id: TtsModelId,
    voice_id: TtsVoiceId,
}

impl MiniMaxTtsConfig {
    pub fn new(model_id: TtsModelId, voice_id: TtsVoiceId) -> Result<Self, TtsError> {
        if !matches!(
            model_id.as_str(),
            MINIMAX_SPEECH_TURBO_MODEL_ID | MINIMAX_SPEECH_HD_MODEL_ID
        ) {
            return Err(TtsError::new(
                TtsErrorKind::Configuration,
                "unsupported MiniMax speech model",
            ));
        }
        Ok(Self { model_id, voice_id })
    }

    pub fn model_id(&self) -> &TtsModelId {
        &self.model_id
    }

    pub fn voice_id(&self) -> &TtsVoiceId {
        &self.voice_id
    }
}
