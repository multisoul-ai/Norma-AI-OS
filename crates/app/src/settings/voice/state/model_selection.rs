use super::super::models::AsrModelManagerState;
use super::super::tts_models::TtsModelManagerState;
use super::{VoiceSettingsState, VoiceSettingsValues};
use voice_sherpa::SpeechError;

impl VoiceSettingsState {
    pub(crate) fn advanced_expanded(&self) -> bool {
        self.advanced_expanded
    }

    pub(crate) fn toggle_advanced_expanded(&mut self) {
        self.advanced_expanded = !self.advanced_expanded;
    }

    pub(crate) fn model_root(&self) -> &std::path::Path {
        &self.model_root
    }

    pub(crate) fn model_manager(&self) -> &AsrModelManagerState {
        &self.model_manager
    }

    pub(crate) fn replace_model_manager(&mut self, manager: AsrModelManagerState) {
        self.model_manager = manager;
    }

    pub(crate) fn model_manager_mut(&mut self) -> &mut AsrModelManagerState {
        &mut self.model_manager
    }

    pub(crate) fn tts_model_root(&self) -> std::path::PathBuf {
        self.model_root.join("tts")
    }
    pub(crate) fn tts_model_manager(&self) -> &TtsModelManagerState {
        &self.tts_model_manager
    }
    pub(crate) fn tts_model_manager_mut(&mut self) -> &mut TtsModelManagerState {
        &mut self.tts_model_manager
    }
    pub(crate) fn replace_tts_model_manager(&mut self, manager: TtsModelManagerState) {
        self.tts_model_manager = manager;
    }
}

pub(crate) fn resolve_available_asr_model(
    values: &VoiceSettingsValues,
    model_root: &std::path::Path,
) -> Result<asr::AvailableAsrModel, SpeechError> {
    let model_id = values.asr_model_id().ok_or_else(|| {
        SpeechError::ModelNotFound(
            "no ASR model selected; install and select a model in Voice Settings".to_string(),
        )
    })?;
    let catalog = asr::builtin_model_catalog();
    let descriptor = catalog.find(model_id).ok_or_else(|| {
        SpeechError::ModelNotFound(format!("unknown ASR model id: {}", model_id.as_str()))
    })?;
    let root = model_root.join(model_id.as_str());
    asr::validate_installation(&root, descriptor)
        .map_err(|error| SpeechError::ModelNotFound(error.to_string()))
}

pub(crate) fn selected_tts_model(
    values: &VoiceSettingsValues,
    model_root: &std::path::Path,
) -> Option<crate::app::dialogue::SelectedTtsModel> {
    let model_id = values.tts_model_id()?.clone();
    let catalog = tts::builtin_model_catalog();
    let descriptor = catalog.find(&model_id)?;
    let voice_id = descriptor
        .resolve_voice_id(values.tts_voice_id().map(tts::TtsVoiceId::as_str))
        .clone();
    Some(crate::app::dialogue::SelectedTtsModel {
        root: model_root.join("tts"),
        model_id,
        voice_id,
    })
}
