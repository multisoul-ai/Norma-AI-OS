use super::{AsrModelManagerState, TtsModelManagerState, VoiceSettingsState};
use std::path::PathBuf;

impl VoiceSettingsState {
    pub(crate) fn reset_model_root_for_e2e(&mut self, model_root: PathBuf) {
        self.model_root = model_root;
        self.model_manager = AsrModelManagerState::unscanned(&asr::builtin_model_catalog());
        self.tts_model_manager = TtsModelManagerState::unscanned(&tts::builtin_model_catalog());
    }
}
