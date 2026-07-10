use gpui::Keystroke;
use std::path::PathBuf;
use storage::{AppStore, SettingsStore};
use voice_sherpa::{AudioInputDevice, SherpaProvider, SpeechError};

mod provider_values;
mod values;
pub(crate) use provider_values::AgentProviderKind;
#[cfg(test)]
pub(crate) use provider_values::SavedProviderProfile;
use provider_values::{
    KEY_AGENT_OLLAMA_API_KEY, KEY_AGENT_OLLAMA_BASE_URL, KEY_AGENT_OLLAMA_MODEL,
    KEY_AGENT_OPENAI_API_KEY, KEY_AGENT_OPENAI_BASE_URL, KEY_AGENT_OPENAI_MODEL,
    KEY_AGENT_PROVIDER,
};
pub(crate) use values::{
    DialogueLlmMode, EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsField, VoiceSettingsValues,
};
use values::{
    KEY_DIALOGUE_LLM_MODE, KEY_DIALOGUE_MODEL, KEY_INPUT_DEVICE_ID, KEY_OLLAMA_ENDPOINT,
    KEY_OLLAMA_MODEL, KEY_PARAFORMER_MODEL_DIR, KEY_REALTIME_ENABLED, KEY_REFINE_MODEL,
    KEY_TRACE_ENABLED,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct VoiceSettingsStatus {
    message: String,
    is_error: bool,
}

#[derive(Clone)]
pub(crate) enum PendingVoiceSettingWrite {
    Set {
        store: SettingsStore,
        key: String,
        value: String,
    },
    Delete {
        store: SettingsStore,
        key: String,
    },
}

impl PendingVoiceSettingWrite {
    pub(crate) fn execute(self) -> storage::StorageResult<()> {
        match self {
            Self::Set { store, key, value } => store.set_string(&key, &value),
            Self::Delete { store, key } => store.delete_string(&key),
        }
    }
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
}

impl VoiceSettingsState {
    pub(crate) fn in_memory_default() -> Self {
        Self::from_sources(SavedVoiceSettings::default(), EnvVoiceSettings::default())
    }

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
        let values = VoiceSettingsValues::resolve(&saved, &env);
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

    pub(crate) fn input_devices(&self) -> &[AudioInputDevice] {
        &self.input_devices
    }

    pub(crate) fn sherpa_provider(&self) -> Result<SherpaProvider, SpeechError> {
        match self.values.paraformer_model_dir.as_deref() {
            Some(path) => SherpaProvider::new(path),
            None => SherpaProvider::from_env_or_default(),
        }
    }

    pub(crate) fn begin_edit(&mut self, field: VoiceSettingsField) {
        self.draft = self.field_value(field);
        self.editing = Some(field);
        self.status = None;
    }

    pub(crate) fn handle_key_down(&mut self, keystroke: &Keystroke) -> bool {
        let Some(field) = self.editing else {
            return false;
        };

        match keystroke.key.as_ref() {
            "escape" => {
                self.cancel_edit();
                true
            }
            "enter" => {
                self.commit_edit(field);
                true
            }
            "backspace" => {
                self.draft.pop();
                true
            }
            "space" if no_command_modifiers(keystroke) => {
                self.draft.push(' ');
                true
            }
            _ if no_command_modifiers(keystroke) => {
                if let Some(text) = keystroke.key_char.as_deref() {
                    self.draft.push_str(text);
                }
                true
            }
            _ => true,
        }
    }

    pub(crate) fn set_paraformer_model_dir(&mut self, path: PathBuf) {
        self.saved.paraformer_model_dir = Some(path.clone());
        self.persist_string(KEY_PARAFORMER_MODEL_DIR, &path.display().to_string());
        self.refresh_values();
    }

    pub(crate) fn clear_paraformer_model_dir(&mut self) {
        self.saved.paraformer_model_dir = None;
        self.delete_string(KEY_PARAFORMER_MODEL_DIR);
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

    pub(crate) fn toggle_realtime_enabled(&mut self) {
        let next = !self.values.realtime_enabled;
        self.saved.realtime_enabled = Some(next);
        self.persist_string(KEY_REALTIME_ENABLED, if next { "true" } else { "false" });
        self.refresh_values();
    }

    pub(crate) fn toggle_dialogue_llm_mode(&mut self) {
        let next = match self.values.dialogue_llm_mode {
            DialogueLlmMode::Mock => DialogueLlmMode::Ollama,
            DialogueLlmMode::Ollama => DialogueLlmMode::Mock,
        };
        self.saved.dialogue_llm_mode = Some(next);
        self.persist_string(KEY_DIALOGUE_LLM_MODE, next.as_str());
        self.refresh_values();
    }

    pub(crate) fn select_agent_provider(&mut self, provider: AgentProviderKind) {
        self.saved.agent_provider = Some(provider);
        self.persist_string(KEY_AGENT_PROVIDER, provider.as_str());
        self.cancel_edit();
        self.refresh_values();
    }

    pub(crate) fn clear_agent_api_key(&mut self) {
        let key = match self.values.agent_provider() {
            AgentProviderKind::Ollama => {
                self.saved.ollama_profile.api_key = None;
                KEY_AGENT_OLLAMA_API_KEY
            }
            AgentProviderKind::OpenAiCompatible => {
                self.saved.openai_profile.api_key = None;
                KEY_AGENT_OPENAI_API_KEY
            }
        };
        self.delete_string(key);
        self.refresh_values();
    }

    pub(super) fn is_editing(&self, field: VoiceSettingsField) -> bool {
        self.editing == Some(field)
    }

    pub(super) fn row_value(&self, field: VoiceSettingsField) -> String {
        if self.editing == Some(field) && field == VoiceSettingsField::AgentApiKey {
            format!("{}|", "•".repeat(self.draft.chars().count()))
        } else if self.editing == Some(field) {
            format!("{}|", self.draft)
        } else if field == VoiceSettingsField::AgentApiKey {
            if self.values.active_provider_profile().api_key().is_some() {
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

    pub(crate) fn take_pending_writes(&mut self) -> Vec<PendingVoiceSettingWrite> {
        if self.persistence_running || self.pending_writes.is_empty() {
            return Vec::new();
        }
        self.persistence_running = true;
        std::mem::take(&mut self.pending_writes)
    }

    pub(crate) fn record_persistence_result(&mut self, result: Result<(), String>) {
        self.persistence_running = false;
        self.status = Some(match result {
            Ok(()) => ok_status("Saved"),
            Err(error) => error_status(format!("Could not save setting: {error}")),
        });
    }

    fn cancel_edit(&mut self) {
        self.editing = None;
        self.draft.clear();
        self.status = None;
    }

    fn commit_edit(&mut self, field: VoiceSettingsField) {
        let value = self.draft.trim().to_string();
        if value.is_empty() && field != VoiceSettingsField::AgentApiKey {
            self.status = Some(error_status("Value cannot be empty"));
            return;
        }

        let key = match field {
            VoiceSettingsField::AgentBaseUrl => match self.values.agent_provider() {
                AgentProviderKind::Ollama => {
                    self.saved.ollama_profile.base_url = Some(value.clone());
                    KEY_AGENT_OLLAMA_BASE_URL
                }
                AgentProviderKind::OpenAiCompatible => {
                    self.saved.openai_profile.base_url = Some(value.clone());
                    KEY_AGENT_OPENAI_BASE_URL
                }
            },
            VoiceSettingsField::AgentModel => match self.values.agent_provider() {
                AgentProviderKind::Ollama => {
                    self.saved.ollama_profile.model = Some(value.clone());
                    KEY_AGENT_OLLAMA_MODEL
                }
                AgentProviderKind::OpenAiCompatible => {
                    self.saved.openai_profile.model = Some(value.clone());
                    KEY_AGENT_OPENAI_MODEL
                }
            },
            VoiceSettingsField::AgentApiKey => match self.values.agent_provider() {
                AgentProviderKind::Ollama => {
                    self.saved.ollama_profile.api_key = non_empty(value.clone());
                    KEY_AGENT_OLLAMA_API_KEY
                }
                AgentProviderKind::OpenAiCompatible => {
                    self.saved.openai_profile.api_key = non_empty(value.clone());
                    KEY_AGENT_OPENAI_API_KEY
                }
            },
            VoiceSettingsField::OllamaEndpoint => {
                self.saved.ollama_endpoint = Some(value.clone());
                KEY_OLLAMA_ENDPOINT
            }
            VoiceSettingsField::PlannerModel => {
                self.saved.planner_model = Some(value.clone());
                KEY_OLLAMA_MODEL
            }
            VoiceSettingsField::DialogueModel => {
                self.saved.dialogue_model = Some(value.clone());
                KEY_DIALOGUE_MODEL
            }
            VoiceSettingsField::RefineModel => {
                self.saved.refine_model = Some(value.clone());
                KEY_REFINE_MODEL
            }
        };
        if value.is_empty() {
            self.delete_string(key);
        } else {
            self.persist_string(key, &value);
        }
        self.editing = None;
        self.draft.clear();
        self.refresh_values();
    }

    fn persist_string(&mut self, key: &str, value: &str) {
        match &self.storage {
            Some(store) => {
                self.pending_writes.push(PendingVoiceSettingWrite::Set {
                    store: store.clone(),
                    key: key.to_string(),
                    value: value.to_string(),
                });
                self.status = Some(ok_status("Saving..."));
            }
            None => self.status = Some(ok_status("Saved for this session")),
        }
    }

    fn delete_string(&mut self, key: &str) {
        match &self.storage {
            Some(store) => {
                self.pending_writes.push(PendingVoiceSettingWrite::Delete {
                    store: store.clone(),
                    key: key.to_string(),
                });
                self.status = Some(ok_status("Saving..."));
            }
            None => self.status = Some(ok_status("Using default for this session")),
        }
    }

    fn refresh_values(&mut self) {
        self.values = VoiceSettingsValues::resolve(&self.saved, &self.env);
    }

    fn field_value(&self, field: VoiceSettingsField) -> String {
        match field {
            VoiceSettingsField::AgentBaseUrl => {
                self.values.active_provider_profile().base_url().to_string()
            }
            VoiceSettingsField::AgentModel => {
                self.values.active_provider_profile().model().to_string()
            }
            VoiceSettingsField::AgentApiKey => String::new(),
            VoiceSettingsField::OllamaEndpoint => self.values.ollama_endpoint().to_string(),
            VoiceSettingsField::PlannerModel => self.values.planner_model().to_string(),
            VoiceSettingsField::DialogueModel => self.values.dialogue_model().to_string(),
            VoiceSettingsField::RefineModel => self.values.refine_model().to_string(),
        }
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn no_command_modifiers(keystroke: &Keystroke) -> bool {
    !keystroke.modifiers.platform && !keystroke.modifiers.control && !keystroke.modifiers.alt
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
#[path = "../../../tests/unit/voice_settings_persistence_tests.rs"]
mod voice_settings_persistence_tests;
