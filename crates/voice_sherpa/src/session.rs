use crate::SpeechError;
use std::path::PathBuf;

pub(crate) struct InnerSession;
impl InnerSession {
    pub(crate) fn finish(self) -> Result<String, SpeechError> {
        unimplemented!("session stub")
    }
    pub(crate) fn cancel(self) {}
}

pub(crate) fn start(_model_dir: PathBuf) -> Result<crate::SpeechSession, SpeechError> {
    unimplemented!("session start stub")
}
