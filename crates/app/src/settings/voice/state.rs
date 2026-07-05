use gpui::Keystroke;
use std::path::PathBuf;
use storage::{AppStore, SettingsStore};
use voice_sherpa::{AudioInputDevice, SherpaProvider, SpeechError};

mod values;
use values::{
    non_empty_option, API_KEY_MASK, KEY_INPUT_DEVICE_ID, KEY_OLLAMA_ENDPOINT, KEY_OLLAMA_MODEL,
    KEY_OPENAI_API_KEY, KEY_OPENAI_CARD_PLANNER_MODEL, KEY_OPENAI_REALTIME_MODEL,
    KEY_OPENAI_SIDECAR_PATH, KEY_PARAFORMER_MODEL_DIR, KEY_REALTIME_ENABLED, KEY_REFINE_MODEL,
    KEY_TRACE_ENABLED,
};
pub(crate) use values::{
    EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsField, VoiceSettingsValues,
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
        }
    }

    pub(crate) fn values(&self) -> &VoiceSettingsValues {
        &self.values
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
        self.draft = if field == VoiceSettingsField::OpenAiApiKey {
            String::new()
        } else {
            self.field_value(field)
        };
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

    pub(crate) fn set_openai_api_key(&mut self, value: String) {
        self.saved.openai_api_key = non_empty_option(Some(value));
        match self.saved.openai_api_key.clone() {
            Some(value) => self.persist_string(KEY_OPENAI_API_KEY, &value),
            None => self.delete_string(KEY_OPENAI_API_KEY),
        }
        self.refresh_values();
    }

    pub(crate) fn set_realtime_model(&mut self, value: String) {
        self.saved.realtime_model = non_empty_option(Some(value));
        match self.saved.realtime_model.clone() {
            Some(value) => self.persist_string(KEY_OPENAI_REALTIME_MODEL, &value),
            None => self.delete_string(KEY_OPENAI_REALTIME_MODEL),
        }
        self.refresh_values();
    }

    pub(crate) fn set_card_planner_model(&mut self, value: String) {
        self.saved.card_planner_model = non_empty_option(Some(value));
        match self.saved.card_planner_model.clone() {
            Some(value) => self.persist_string(KEY_OPENAI_CARD_PLANNER_MODEL, &value),
            None => self.delete_string(KEY_OPENAI_CARD_PLANNER_MODEL),
        }
        self.refresh_values();
    }

    pub(crate) fn set_sidecar_path(&mut self, path: PathBuf) {
        self.saved.sidecar_path = Some(path.clone());
        self.persist_string(KEY_OPENAI_SIDECAR_PATH, &path.display().to_string());
        self.refresh_values();
    }

    pub(super) fn is_editing(&self, field: VoiceSettingsField) -> bool {
        self.editing == Some(field)
    }

    pub(super) fn row_value(&self, field: VoiceSettingsField) -> String {
        if self.editing == Some(field) {
            if field == VoiceSettingsField::OpenAiApiKey && !self.draft.is_empty() {
                format!("{API_KEY_MASK}|")
            } else {
                format!("{}|", self.draft)
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

    fn commit_edit(&mut self, field: VoiceSettingsField) {
        let value = self.draft.trim().to_string();
        if value.is_empty() {
            if self.clear_empty_realtime_field(field) {
                self.editing = None;
                self.draft.clear();
            } else {
                self.status = Some(error_status("Value cannot be empty"));
            }
            return;
        }

        match field {
            VoiceSettingsField::OpenAiApiKey => {
                self.set_openai_api_key(value);
                self.editing = None;
                self.draft.clear();
                return;
            }
            VoiceSettingsField::RealtimeModel => {
                self.set_realtime_model(value);
                self.editing = None;
                self.draft.clear();
                return;
            }
            VoiceSettingsField::CardPlannerModel => {
                self.set_card_planner_model(value);
                self.editing = None;
                self.draft.clear();
                return;
            }
            VoiceSettingsField::SidecarPath => {
                self.set_sidecar_path(PathBuf::from(value));
                self.editing = None;
                self.draft.clear();
                return;
            }
            VoiceSettingsField::OllamaEndpoint
            | VoiceSettingsField::PlannerModel
            | VoiceSettingsField::RefineModel => {}
        }

        let key = match field {
            VoiceSettingsField::OllamaEndpoint => {
                self.saved.ollama_endpoint = Some(value.clone());
                KEY_OLLAMA_ENDPOINT
            }
            VoiceSettingsField::PlannerModel => {
                self.saved.planner_model = Some(value.clone());
                KEY_OLLAMA_MODEL
            }
            VoiceSettingsField::RefineModel => {
                self.saved.refine_model = Some(value.clone());
                KEY_REFINE_MODEL
            }
            VoiceSettingsField::OpenAiApiKey
            | VoiceSettingsField::RealtimeModel
            | VoiceSettingsField::CardPlannerModel
            | VoiceSettingsField::SidecarPath => unreachable!("realtime fields return above"),
        };
        self.persist_string(key, &value);
        self.editing = None;
        self.draft.clear();
        self.refresh_values();
    }

    fn clear_empty_realtime_field(&mut self, field: VoiceSettingsField) -> bool {
        match field {
            VoiceSettingsField::OpenAiApiKey => {
                self.set_openai_api_key(String::new());
                true
            }
            VoiceSettingsField::RealtimeModel => {
                self.set_realtime_model(String::new());
                true
            }
            VoiceSettingsField::CardPlannerModel => {
                self.set_card_planner_model(String::new());
                true
            }
            VoiceSettingsField::SidecarPath => {
                self.saved.sidecar_path = None;
                self.delete_string(KEY_OPENAI_SIDECAR_PATH);
                self.refresh_values();
                true
            }
            VoiceSettingsField::OllamaEndpoint
            | VoiceSettingsField::PlannerModel
            | VoiceSettingsField::RefineModel => false,
        }
    }

    fn persist_string(&mut self, key: &str, value: &str) {
        self.status = match &self.storage {
            Some(store) => match store.set_string(key, value) {
                Ok(()) => Some(ok_status("Saved")),
                Err(error) => Some(error_status(format!("Could not save setting: {error}"))),
            },
            None => Some(ok_status("Saved for this session")),
        };
    }

    fn delete_string(&mut self, key: &str) {
        self.status = match &self.storage {
            Some(store) => match store.delete_string(key) {
                Ok(()) => Some(ok_status("Using default")),
                Err(error) => Some(error_status(format!("Could not clear setting: {error}"))),
            },
            None => Some(ok_status("Using default for this session")),
        };
    }

    fn refresh_values(&mut self) {
        self.values = VoiceSettingsValues::resolve(&self.saved, &self.env);
    }

    fn field_value(&self, field: VoiceSettingsField) -> String {
        match field {
            VoiceSettingsField::OllamaEndpoint => self.values.ollama_endpoint().to_string(),
            VoiceSettingsField::PlannerModel => self.values.planner_model().to_string(),
            VoiceSettingsField::RefineModel => self.values.refine_model().to_string(),
            VoiceSettingsField::OpenAiApiKey => {
                if self.values.openai_api_key().is_some() {
                    API_KEY_MASK.to_string()
                } else {
                    "Not set".to_string()
                }
            }
            VoiceSettingsField::RealtimeModel => self
                .values
                .realtime_model()
                .unwrap_or("Not set")
                .to_string(),
            VoiceSettingsField::CardPlannerModel => self
                .values
                .card_planner_model()
                .unwrap_or("Not set")
                .to_string(),
            VoiceSettingsField::SidecarPath => self
                .values
                .sidecar_path()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "Not set".to_string()),
        }
    }
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
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_realtime_tests.rs"]
mod realtime_tests;
