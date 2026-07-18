use super::super::models::AsrModelManagerState;
use super::super::tts_models::TtsModelManagerState;
use super::{VoiceSettingsState, VoiceSettingsValues};
use voice_sherpa::SpeechError;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum VoiceSetupDestination {
    #[default]
    Overview,
    AsrModels,
    SpokenReplies,
}

impl VoiceSettingsState {
    pub(crate) fn advanced_expanded(&self) -> bool {
        self.advanced_expanded
    }

    pub(crate) fn toggle_advanced_expanded(&mut self) {
        self.advanced_expanded = !self.advanced_expanded;
    }

    pub(crate) fn setup_models_expanded(&self) -> bool {
        self.setup_destination == VoiceSetupDestination::AsrModels
    }

    #[cfg(test)]
    pub(crate) fn setup_focuses_spoken_replies(&self) -> bool {
        self.setup_destination == VoiceSetupDestination::SpokenReplies
    }

    pub(crate) fn toggle_setup_models_expanded(&mut self) {
        self.setup_destination = if self.setup_models_expanded() {
            VoiceSetupDestination::Overview
        } else {
            VoiceSetupDestination::AsrModels
        };
    }

    pub(crate) fn open_setup_destination(&mut self, destination: VoiceSetupDestination) {
        self.setup_destination = destination;
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

    pub(crate) fn report_tts_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status = Some(if is_error {
            super::error_status(message)
        } else {
            super::ok_status(message)
        });
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
