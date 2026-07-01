use gpui::Keystroke;
use std::path::PathBuf;
use storage::{AppStore, SettingsStore};
use voice::OllamaVoicePlanner;
use voice_sherpa::{SherpaProvider, SpeechError};

pub(crate) const DEFAULT_OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434";
const KEY_OLLAMA_ENDPOINT: &str = "voice.ollama.endpoint";
const KEY_OLLAMA_MODEL: &str = "voice.ollama.model";
const KEY_REFINE_MODEL: &str = "voice.refine.model";
const KEY_PARAFORMER_MODEL_DIR: &str = "voice.paraformer.model_dir";
const KEY_TRACE_ENABLED: &str = "voice.trace_enabled";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SavedVoiceSettings {
    pub(crate) ollama_endpoint: Option<String>,
    pub(crate) planner_model: Option<String>,
    pub(crate) refine_model: Option<String>,
    pub(crate) paraformer_model_dir: Option<PathBuf>,
    pub(crate) trace_enabled: Option<bool>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct EnvVoiceSettings {
    pub(crate) ollama_endpoint: Option<String>,
    pub(crate) planner_model: Option<String>,
    pub(crate) refine_model: Option<String>,
    pub(crate) paraformer_model_dir: Option<PathBuf>,
    pub(crate) trace_enabled: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VoiceSettingsValues {
    ollama_endpoint: String,
    planner_model: String,
    refine_model: String,
    paraformer_model_dir: Option<PathBuf>,
    trace_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceSettingsField {
    OllamaEndpoint,
    PlannerModel,
    RefineModel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VoiceSettingsStatus {
    message: String,
    is_error: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct VoiceSettingsState {
    values: VoiceSettingsValues,
    saved: SavedVoiceSettings,
    env: EnvVoiceSettings,
    storage: Option<SettingsStore>,
    editing: Option<VoiceSettingsField>,
    draft: String,
    status: Option<VoiceSettingsStatus>,
}

impl VoiceSettingsValues {
    pub(crate) fn resolve(saved: &SavedVoiceSettings, env: &EnvVoiceSettings) -> Self {
        let endpoint = first_non_empty(
            saved.ollama_endpoint.as_deref(),
            env.ollama_endpoint.as_deref(),
            DEFAULT_OLLAMA_ENDPOINT,
        );
        let planner_model = first_non_empty(
            saved.planner_model.as_deref(),
            env.planner_model.as_deref(),
            OllamaVoicePlanner::default_model(),
        );
        let refine_model = saved
            .refine_model
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env.refine_model
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
            })
            .map(str::trim)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| planner_model.clone());
        let paraformer_model_dir = saved
            .paraformer_model_dir
            .clone()
            .or_else(|| env.paraformer_model_dir.clone());
        let trace_enabled = saved
            .trace_enabled
            .unwrap_or_else(|| trace_enabled_from_value(env.trace_enabled.as_deref()));

        Self {
            ollama_endpoint: endpoint,
            planner_model,
            refine_model,
            paraformer_model_dir,
            trace_enabled,
        }
    }

    pub(crate) fn ollama_endpoint(&self) -> &str {
        &self.ollama_endpoint
    }

    pub(crate) fn planner_model(&self) -> &str {
        &self.planner_model
    }

    pub(crate) fn refine_model(&self) -> &str {
        &self.refine_model
    }

    pub(crate) fn paraformer_model_dir(&self) -> Option<&std::path::Path> {
        self.paraformer_model_dir.as_deref()
    }

    pub(crate) fn trace_enabled(&self) -> bool {
        self.trace_enabled
    }
}

impl EnvVoiceSettings {
    fn from_env() -> Self {
        Self {
            ollama_endpoint: std::env::var("MULTISOUL_OLLAMA_ENDPOINT").ok(),
            planner_model: std::env::var("MULTISOUL_OLLAMA_MODEL").ok(),
            refine_model: std::env::var("MULTISOUL_REFINE_MODEL").ok(),
            paraformer_model_dir: std::env::var("MULTISOUL_PARAFORMER_MODEL_DIR")
                .ok()
                .map(PathBuf::from),
            trace_enabled: std::env::var("MULTISOUL_VOICE_TRACE").ok(),
        }
    }
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
            editing: None,
            draft: String::new(),
            status: None,
        }
    }

    pub(crate) fn values(&self) -> &VoiceSettingsValues {
        &self.values
    }

    pub(crate) fn sherpa_provider(&self) -> Result<SherpaProvider, SpeechError> {
        match self.values.paraformer_model_dir.as_deref() {
            Some(path) => SherpaProvider::new(path),
            None => SherpaProvider::from_env_or_default(),
        }
    }

    pub(crate) fn begin_edit(&mut self, field: VoiceSettingsField) {
        self.draft = self.field_value(field).to_string();
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

    pub(crate) fn toggle_trace(&mut self) {
        let next = !self.values.trace_enabled;
        self.saved.trace_enabled = Some(next);
        self.persist_string(KEY_TRACE_ENABLED, if next { "true" } else { "false" });
        self.refresh_values();
    }

    pub(super) fn is_editing(&self, field: VoiceSettingsField) -> bool {
        self.editing == Some(field)
    }

    pub(super) fn row_value(&self, field: VoiceSettingsField) -> String {
        if self.editing == Some(field) {
            format!("{}|", self.draft)
        } else {
            self.field_value(field).to_string()
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
            self.status = Some(error_status("Value cannot be empty"));
            return;
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
        };
        self.persist_string(key, &value);
        self.editing = None;
        self.draft.clear();
        self.refresh_values();
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

    fn field_value(&self, field: VoiceSettingsField) -> &str {
        match field {
            VoiceSettingsField::OllamaEndpoint => self.values.ollama_endpoint(),
            VoiceSettingsField::PlannerModel => self.values.planner_model(),
            VoiceSettingsField::RefineModel => self.values.refine_model(),
        }
    }
}

impl SavedVoiceSettings {
    fn load(store: &SettingsStore) -> storage::StorageResult<Self> {
        Ok(Self {
            ollama_endpoint: non_empty_option(store.get_string(KEY_OLLAMA_ENDPOINT)?),
            planner_model: non_empty_option(store.get_string(KEY_OLLAMA_MODEL)?),
            refine_model: non_empty_option(store.get_string(KEY_REFINE_MODEL)?),
            paraformer_model_dir: non_empty_option(store.get_string(KEY_PARAFORMER_MODEL_DIR)?)
                .map(PathBuf::from),
            trace_enabled: store
                .get_string(KEY_TRACE_ENABLED)?
                .as_deref()
                .and_then(stored_bool),
        })
    }
}

impl VoiceSettingsField {
    pub(super) fn index(self) -> usize {
        match self {
            Self::OllamaEndpoint => 0,
            Self::PlannerModel => 1,
            Self::RefineModel => 2,
        }
    }
}

fn first_non_empty(saved: Option<&str>, env: Option<&str>, default: &str) -> String {
    saved
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env.filter(|value| !value.trim().is_empty()))
        .map(str::trim)
        .unwrap_or(default)
        .to_string()
}

fn non_empty_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn stored_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn trace_enabled_from_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
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
