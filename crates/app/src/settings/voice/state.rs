use super::models::AsrModelManagerState;
use super::tts_models::TtsModelManagerState;
use asr::AsrModelId;
use std::path::PathBuf;
use storage::{AppStore, SettingsStore};
use tts::{TtsModelId, TtsVoiceId};
use voice_sherpa::{AudioInputDevice, SpeechError};
mod announcements;
mod builders;
mod credentials;
#[cfg(feature = "e2e-test-support")]
mod e2e;
mod migration;
mod model_selection;
mod persistence;
mod provider_discovery;
mod provider_values;
mod readiness_state;
mod values;
use credentials::CredentialWriteRollback;
pub(crate) use model_selection::resolve_available_asr_model;
pub(crate) use persistence::PendingVoiceSettingWrite;
pub(crate) use provider_discovery::ProviderDiscoveryStatus;
pub(crate) use provider_values::AgentProviderKind;
#[cfg(test)]
pub(crate) use provider_values::SavedProviderProfile;
use provider_values::{
    KEY_AGENT_OLLAMA_BASE_URL, KEY_AGENT_OLLAMA_MODEL, KEY_AGENT_OPENAI_BASE_URL,
    KEY_AGENT_OPENAI_MODEL, KEY_AGENT_PROVIDER,
};
#[cfg(test)]
pub(crate) use values::DialogueLlmMode;
pub(crate) use values::{
    DictationTrigger, EnvVoiceSettings, SavedVoiceSettings, VoiceInteractionMode,
    VoiceSettingsField, VoiceSettingsValues,
};
use values::{
    KEY_ASR_MODEL_ID, KEY_INPUT_DEVICE_ID, KEY_INTERACTION_MODE, KEY_SINGLE_VOICE_COMMAND_TRIGGER,
    KEY_TRACE_ENABLED, KEY_TTS_MODEL_ID, KEY_TTS_VOICE_ID, KEY_VOICE_SETUP_COMPLETED,
};
#[derive(Clone, Debug, Eq, PartialEq)]
struct VoiceSettingsStatus {
    message: String,
    is_error: bool,
}
#[derive(Clone)]
pub(crate) struct VoiceSettingsState {
    values: VoiceSettingsValues,
    saved: SavedVoiceSettings,
    env: EnvVoiceSettings,
    storage: Option<SettingsStore>,
    input_devices: Vec<AudioInputDevice>,
    editing: Option<VoiceSettingsField>,
    draft: String,
    status: Option<VoiceSettingsStatus>,
    pending_writes: Vec<PendingVoiceSettingWrite>,
    persistence_running: bool,
    credential_write_rollback: Option<CredentialWriteRollback>,
    model_root: PathBuf,
    model_manager: AsrModelManagerState,
    tts_model_manager: TtsModelManagerState,
    microphone_requirement: super::VoiceRequirementState,
    provider_requirement: super::VoiceRequirementState,
    provider_discovery: ProviderDiscoveryStatus,
    provider_discovery_generation: u64,
    setup_was_completed: bool,
    #[cfg(feature = "e2e-test-support")]
    readiness_override: Option<super::VoiceEntryState>,
    reviewing_setup: bool,
    input_device_picker_expanded: bool,
    advanced_expanded: bool,
}
impl VoiceSettingsState {
    pub(crate) fn load_default() -> Self {
        match AppStore::open_default() {
            Ok(store) => Self::load_from_store(store.settings_store()),
            Err(error) => {
                let mut state =
                    Self::from_sources(SavedVoiceSettings::default(), EnvVoiceSettings::from_env());
                state.status = Some(error_status(format!(
                    "Could not initialize voice settings: {error}"
                )));
                state
            }
        }
    }

    pub(crate) fn load_from_store(store: SettingsStore) -> Self {
        match SavedVoiceSettings::load(&store) {
            Ok(saved) => {
                let mut state = Self::from_sources(saved, EnvVoiceSettings::from_env());
                state.storage = Some(store);
                state.queue_legacy_single_voice_command_migration();
                state.queue_legacy_api_key_migrations();
                state
            }
            Err(error) => {
                let mut state =
                    Self::from_sources(SavedVoiceSettings::default(), EnvVoiceSettings::from_env());
                state.storage = Some(store);
                state.status = Some(error_status(format!(
                    "Could not load voice settings: {error}"
                )));
                state
            }
        }
    }

    pub(crate) fn from_sources(saved: SavedVoiceSettings, env: EnvVoiceSettings) -> Self {
        let model_root = asr::default_model_installation_root().unwrap_or_default();
        Self::from_sources_with_model_root(saved, env, model_root)
    }

    pub(crate) fn from_sources_with_model_root(
        saved: SavedVoiceSettings,
        env: EnvVoiceSettings,
        model_root: PathBuf,
    ) -> Self {
        let values = VoiceSettingsValues::resolve(&saved, &env);
        let setup_was_completed = saved.setup_completed.unwrap_or(false);
        let model_manager = AsrModelManagerState::unscanned(&asr::builtin_model_catalog());
        let tts_model_manager = TtsModelManagerState::unscanned(&tts::builtin_model_catalog());
        Self {
            values,
            saved,
            env,
            storage: None,
            input_devices: Vec::new(),
            editing: None,
            draft: String::new(),
            status: None,
            pending_writes: Vec::new(),
            persistence_running: false,
            credential_write_rollback: None,
            model_root,
            model_manager,
            tts_model_manager,
            microphone_requirement: super::VoiceRequirementState::Missing,
            provider_requirement: super::VoiceRequirementState::Missing,
            provider_discovery: ProviderDiscoveryStatus::Idle,
            provider_discovery_generation: 0,
            setup_was_completed,
            #[cfg(feature = "e2e-test-support")]
            readiness_override: None,
            reviewing_setup: false,
            input_device_picker_expanded: false,
            advanced_expanded: false,
        }
    }

    pub(crate) fn values(&self) -> &VoiceSettingsValues {
        &self.values
    }

    pub(crate) fn editing_field(&self) -> Option<VoiceSettingsField> {
        self.editing
    }

    pub(crate) fn active_agent_model(&self) -> &str {
        self.values.active_provider_profile().model()
    }

    pub(crate) fn reviewing_setup(&self) -> bool {
        self.reviewing_setup
    }

    pub(crate) fn set_reviewing_setup(&mut self, reviewing: bool) {
        self.reviewing_setup = reviewing;
    }

    pub(crate) fn input_devices(&self) -> &[AudioInputDevice] {
        &self.input_devices
    }

    pub(crate) fn input_device_picker_expanded(&self) -> bool {
        self.input_device_picker_expanded
    }

    pub(crate) fn toggle_input_device_picker(&mut self) {
        self.input_device_picker_expanded = !self.input_device_picker_expanded;
    }

    pub(crate) fn close_input_device_picker(&mut self) {
        self.input_device_picker_expanded = false;
    }

    #[cfg(test)]
    pub(crate) fn begin_edit(&mut self, field: VoiceSettingsField) {
        self.draft = self.field_value(field);
        self.editing = Some(field);
        self.status = None;
    }

    pub(crate) fn begin_edit_with_value(&mut self, field: VoiceSettingsField, value: String) {
        self.editing = Some(field);
        self.draft = value;
        self.status = None;
    }

    pub(crate) fn update_edit_value(&mut self, field: VoiceSettingsField, value: String) {
        if self.editing == Some(field) {
            self.draft = value;
        }
    }

    pub(crate) fn auto_save_edit_value(
        &mut self,
        field: VoiceSettingsField,
        value: String,
    ) -> Option<String> {
        if self.editing != Some(field) {
            return None;
        }
        self.draft = value;
        self.save_edit(field, false)
    }

    pub(crate) fn commit_edit_value(
        &mut self,
        field: VoiceSettingsField,
        value: String,
    ) -> Option<String> {
        self.editing = Some(field);
        self.draft = value;
        self.commit_edit(field)
    }

    pub(crate) fn cancel_edit_field(&mut self, field: VoiceSettingsField) {
        if self.editing == Some(field) {
            self.cancel_edit();
        }
    }

    pub(crate) fn select_interaction_mode(&mut self, mode: VoiceInteractionMode) {
        self.saved.interaction_mode = Some(mode);
        self.persist_string(KEY_INTERACTION_MODE, mode.as_str());
        self.refresh_values();
    }

    pub(crate) fn select_dictation_trigger(&mut self, trigger: DictationTrigger) {
        self.saved.dictation_trigger = Some(trigger);
        self.persist_string(KEY_SINGLE_VOICE_COMMAND_TRIGGER, trigger.as_str());
        self.refresh_values();
    }

    pub(crate) fn select_asr_model(&mut self, model_id: Option<AsrModelId>) {
        self.saved.asr_model_id = model_id.clone();
        match model_id {
            Some(model_id) => self.persist_string(KEY_ASR_MODEL_ID, model_id.as_str()),
            None => self.delete_string(KEY_ASR_MODEL_ID),
        }
        self.refresh_values();
    }

    pub(crate) fn select_tts_model_and_voice(
        &mut self,
        model_id: Option<TtsModelId>,
        voice_id: Option<TtsVoiceId>,
    ) {
        self.saved.tts_model_id = model_id.clone();
        self.saved.tts_voice_id = voice_id.clone();
        match model_id {
            Some(id) => self.persist_string(KEY_TTS_MODEL_ID, id.as_str()),
            None => self.delete_string(KEY_TTS_MODEL_ID),
        }
        match voice_id {
            Some(id) => self.persist_string(KEY_TTS_VOICE_ID, id.as_str()),
            None => self.delete_string(KEY_TTS_VOICE_ID),
        }
        self.refresh_values();
    }

    pub(crate) fn select_input_device(&mut self, id: Option<String>) {
        self.saved.input_device_id = id
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        match self.saved.input_device_id.clone() {
            Some(id) => self.persist_string(KEY_INPUT_DEVICE_ID, &id),
            None => self.delete_string(KEY_INPUT_DEVICE_ID),
        }
        self.refresh_values();
    }

    pub(crate) fn refresh_input_devices(&mut self) {
        self.refresh_input_devices_from_result(voice_sherpa::list_audio_input_devices());
    }

    pub(crate) fn refresh_input_devices_from_result(
        &mut self,
        result: Result<Vec<AudioInputDevice>, SpeechError>,
    ) {
        match result {
            Ok(devices) => {
                self.input_devices = devices;
                self.status = Some(ok_status("Input devices refreshed"));
            }
            Err(error) => {
                self.status = Some(error_status(format!(
                    "Could not refresh input devices: {error}"
                )));
            }
        }
    }

    pub(crate) fn toggle_trace(&mut self) {
        let next = !self.values.trace_enabled;
        self.saved.trace_enabled = Some(next);
        self.persist_string(KEY_TRACE_ENABLED, if next { "true" } else { "false" });
        self.refresh_values();
    }

    pub(crate) fn select_agent_provider(&mut self, provider: AgentProviderKind) {
        self.saved.agent_provider = Some(provider);
        self.persist_string(KEY_AGENT_PROVIDER, provider.as_str());
        self.cancel_edit();
        self.invalidate_provider_requirement();
        self.refresh_values();
    }

    pub(crate) fn clear_agent_api_key(&mut self) {
        self.cancel_edit_field(VoiceSettingsField::ApiKey);
        self.clear_active_provider_credential();
        self.refresh_values();
    }

    #[cfg(test)]
    pub(super) fn row_value(&self, field: VoiceSettingsField) -> String {
        if self.editing == Some(field) && field == VoiceSettingsField::ApiKey {
            format!("{}|", "•".repeat(self.draft.chars().count()))
        } else if self.editing == Some(field) {
            format!("{}|", self.draft)
        } else if field == VoiceSettingsField::ApiKey {
            if self
                .values
                .active_provider_profile()
                .credential_configured()
            {
                "Configured ••••••••".to_string()
            } else {
                "Not configured".to_string()
            }
        } else {
            self.field_value(field)
        }
    }

    pub(super) fn status(&self) -> Option<(&str, bool)> {
        self.status
            .as_ref()
            .map(|status| (status.message.as_str(), status.is_error))
    }

    fn cancel_edit(&mut self) {
        self.editing = None;
        self.draft.clear();
        self.status = None;
    }

    fn commit_edit(&mut self, field: VoiceSettingsField) -> Option<String> {
        self.save_edit(field, true)
    }

    fn save_edit(&mut self, field: VoiceSettingsField, finish_editing: bool) -> Option<String> {
        let value = self.draft.trim().to_string();
        if value.is_empty() {
            if field == VoiceSettingsField::ApiKey {
                if finish_editing {
                    self.editing = None;
                    self.draft.clear();
                }
                self.status = Some(ok_status("API Key unchanged; use Clear to remove it"));
            } else {
                self.status = Some(error_status("Value cannot be empty"));
            }
            return None;
        }

        let key = match field {
            VoiceSettingsField::BaseUrl => match self.values.agent_provider() {
                AgentProviderKind::Ollama => {
                    self.saved.ollama_profile.base_url = Some(value.clone());
                    KEY_AGENT_OLLAMA_BASE_URL
                }
                AgentProviderKind::OpenAiCompatible => {
                    self.saved.openai_profile.base_url = Some(value.clone());
                    KEY_AGENT_OPENAI_BASE_URL
                }
            },
            VoiceSettingsField::Model => match self.values.agent_provider() {
                AgentProviderKind::Ollama => {
                    self.saved.ollama_profile.model = Some(value.clone());
                    KEY_AGENT_OLLAMA_MODEL
                }
                AgentProviderKind::OpenAiCompatible => {
                    self.saved.openai_profile.model = Some(value.clone());
                    KEY_AGENT_OPENAI_MODEL
                }
            },
            VoiceSettingsField::ApiKey => {
                self.save_active_provider_credential(value.clone());
                if finish_editing {
                    self.editing = None;
                    self.draft.clear();
                }
                self.refresh_values();
                return Some(value);
            }
        };
        self.persist_string(key, &value);
        self.invalidate_provider_requirement();
        if finish_editing {
            self.editing = None;
            self.draft.clear();
        }
        self.refresh_values();
        Some(value)
    }

    fn persist_string(&mut self, key: &str, value: &str) {
        if let Some(store) = self.storage.clone() {
            self.queue_pending_write(PendingVoiceSettingWrite::Set {
                store,
                key: key.to_string(),
                value: value.to_string(),
            });
            self.status = Some(ok_status("Saving..."));
        } else {
            self.status = Some(ok_status("Saved for this session"));
        }
    }

    fn delete_string(&mut self, key: &str) {
        if let Some(store) = self.storage.clone() {
            self.queue_pending_write(PendingVoiceSettingWrite::Delete {
                store,
                key: key.to_string(),
            });
            self.status = Some(ok_status("Saving..."));
        } else {
            self.status = Some(ok_status("Using default for this session"));
        }
    }

    fn queue_pending_write(&mut self, write: PendingVoiceSettingWrite) {
        self.pending_writes
            .retain(|pending| pending.key() != write.key());
        self.pending_writes.push(write);
    }

    fn refresh_values(&mut self) {
        self.values = VoiceSettingsValues::resolve(&self.saved, &self.env);
    }

    fn field_value(&self, field: VoiceSettingsField) -> String {
        match field {
            VoiceSettingsField::BaseUrl => {
                self.values.active_provider_profile().base_url().to_string()
            }
            VoiceSettingsField::Model => self.values.active_provider_profile().model().to_string(),
            VoiceSettingsField::ApiKey => String::new(),
        }
    }

    pub(crate) fn editor_value(&self, field: VoiceSettingsField) -> String {
        self.field_value(field)
    }
}

fn ok_status(message: impl Into<String>) -> VoiceSettingsStatus {
    VoiceSettingsStatus {
        message: message.into(),
        is_error: false,
    }
}

fn error_status(message: impl Into<String>) -> VoiceSettingsStatus {
    VoiceSettingsStatus {
        message: message.into(),
        is_error: true,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_tests.rs"]
mod voice_settings_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_asr_tests.rs"]
mod voice_settings_asr_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_persistence_tests.rs"]
mod voice_settings_persistence_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_provider_persistence_tests.rs"]
mod voice_settings_provider_persistence_tests;
