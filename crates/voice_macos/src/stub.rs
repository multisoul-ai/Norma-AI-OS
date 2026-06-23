use crate::SpeechError;

pub(crate) struct PlatformSpeechSession;

impl PlatformSpeechSession {
    pub(crate) fn start(_: String) -> Result<Self, SpeechError> {
        Err(SpeechError::Provider(
            "Apple Speech is only available on macOS".to_string(),
        ))
    }

    pub(crate) fn finish(&mut self) -> Result<String, SpeechError> {
        Err(SpeechError::NotRecording)
    }

    pub(crate) fn cancel(&mut self) {}
}
