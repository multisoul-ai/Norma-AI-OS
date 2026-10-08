use super::super::models::AsrModelManagerState;
use super::super::tts_models::TtsModelManagerState;
use super::{VoiceSettingsState, VoiceSettingsValues};
use asr::AsrModelId;
use voice_sherpa::SpeechError;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum VoiceSetupDestination {
    #[default]
    Overview,
    AsrOnDevice,
    AsrModels,
    AsrAzure,
    AgentProvider,
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

    pub(crate) fn setup_asr_source(&self) -> asr::AsrProviderId {
        match self.setup_destination {
            VoiceSetupDestination::AsrOnDevice | VoiceSetupDestination::AsrModels => {
                asr::AsrProviderId::LocalSherpa
            }
            VoiceSetupDestination::AsrAzure => asr::AsrProviderId::CloudAzure,
            VoiceSetupDestination::Overview
            | VoiceSetupDestination::AgentProvider
            | VoiceSetupDestination::SpokenReplies => self.values.asr_provider(),
        }
    }

    #[cfg(test)]
    pub(crate) fn setup_focuses_spoken_replies(&self) -> bool {
        self.setup_destination == VoiceSetupDestination::SpokenReplies
    }

    pub(crate) fn toggle_setup_models_expanded(&mut self) {
        self.setup_destination = if self.setup_models_expanded() {
            VoiceSetupDestination::AsrOnDevice
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

    pub(crate) fn complete_asr_model_install(&mut self, id: AsrModelId) -> bool {
        self.model_manager.mark_ready(&id);
        if self.values.asr_model_id().is_some() {
            return false;
        }
        self.select_asr_model(Some(id));
        true
    }

    pub(crate) fn should_restart_for_asr_model_selection(&self, next: &AsrModelId) -> bool {
        self.values.asr_provider() == asr::AsrProviderId::LocalSherpa
            && self
                .values
                .asr_model_id()
                .is_some_and(|current| current != next)
    }

    pub(crate) fn begin_asr_model_selection(&mut self) -> u64 {
        self.asr_model_selection_generation = self.asr_model_selection_generation.wrapping_add(1);
        self.asr_model_selection_generation
    }

    pub(crate) fn accepts_asr_model_selection_generation(&self, generation: u64) -> bool {
        self.asr_model_selection_generation == generation
    }

    pub(crate) fn begin_asr_model_scan(&mut self) -> u64 {
        self.asr_model_scan_generation = self.asr_model_scan_generation.wrapping_add(1);
        self.asr_model_scan_generation
    }

    pub(crate) fn accepts_asr_model_scan(&self, generation: u64) -> bool {
        self.asr_model_scan_generation == generation
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

    pub(crate) fn report_tts_status(
        &mut self,
        message: impl Into<ui::i18n::LocalizedText>,
        is_error: bool,
    ) {
        if self.values.desktop_mode() == super::DesktopVoiceMode::Doubao {
            return;
        }
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
