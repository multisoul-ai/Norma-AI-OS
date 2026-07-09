use super::VoiceSettingsState;
use std::fmt;
use std::path::{Path, PathBuf};
use storage::SettingsStore;
use voice::OllamaVoicePlanner;

pub(crate) const DEFAULT_OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434";
pub(super) const KEY_OLLAMA_ENDPOINT: &str = "voice.ollama.endpoint";
pub(super) const KEY_OLLAMA_MODEL: &str = "voice.ollama.model";
pub(super) const KEY_REFINE_MODEL: &str = "voice.refine.model";
pub(super) const KEY_PARAFORMER_MODEL_DIR: &str = "voice.paraformer.model_dir";
pub(super) const KEY_INPUT_DEVICE_ID: &str = "voice.input_device_id";
pub(super) const KEY_TRACE_ENABLED: &str = "voice.trace_enabled";
pub(super) const KEY_REALTIME_ENABLED: &str = "voice.realtime.enabled";
pub(super) const KEY_OPENAI_API_KEY: &str = "voice.openai.api_key";
pub(super) const KEY_OPENAI_REALTIME_MODEL: &str = "voice.openai.realtime_model";
pub(super) const KEY_OPENAI_CARD_PLANNER_MODEL: &str = "voice.openai.card_planner_model";
pub(super) const KEY_OPENAI_SIDECAR_PATH: &str = "voice.openai.sidecar_path";

#[derive(Clone, Default, Eq, PartialEq)]
pub(crate) struct SavedVoiceSettings {
    pub(crate) ollama_endpoint: Option<String>,
    pub(crate) planner_model: Option<String>,
    pub(crate) refine_model: Option<String>,
    pub(crate) paraformer_model_dir: Option<PathBuf>,
    pub(crate) input_device_id: Option<String>,
    pub(crate) trace_enabled: Option<bool>,
    pub(crate) realtime_enabled: Option<bool>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct EnvVoiceSettings {
    pub(crate) ollama_endpoint: Option<String>,
    pub(crate) planner_model: Option<String>,
    pub(crate) refine_model: Option<String>,
    pub(crate) paraformer_model_dir: Option<PathBuf>,
    pub(crate) input_device_id: Option<String>,
    pub(crate) trace_enabled: Option<String>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct VoiceSettingsValues {
    pub(crate) ollama_endpoint: String,
    pub(crate) planner_model: String,
    pub(crate) refine_model: String,
    pub(crate) paraformer_model_dir: Option<PathBuf>,
    pub(crate) input_device_id: Option<String>,
    pub(crate) trace_enabled: bool,
    pub(crate) realtime_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceSettingsField {
    OllamaEndpoint,
    PlannerModel,
    RefineModel,
}

impl fmt::Debug for SavedVoiceSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedVoiceSettings")
            .field("ollama_endpoint", &self.ollama_endpoint)
            .field("planner_model", &self.planner_model)
            .field("refine_model", &self.refine_model)
            .field("paraformer_model_dir", &self.paraformer_model_dir)
            .field("input_device_id", &self.input_device_id)
            .field("trace_enabled", &self.trace_enabled)
            .field("realtime_enabled", &self.realtime_enabled)
            .finish()
    }
}

impl fmt::Debug for VoiceSettingsValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceSettingsValues")
            .field("ollama_endpoint", &self.ollama_endpoint)
            .field("planner_model", &self.planner_model)
            .field("refine_model", &self.refine_model)
            .field("paraformer_model_dir", &self.paraformer_model_dir)
            .field("input_device_id", &self.input_device_id)
            .field("trace_enabled", &self.trace_enabled)
            .field("realtime_enabled", &self.realtime_enabled)
            .finish()
    }
}

impl fmt::Debug for VoiceSettingsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceSettingsState")
            .field("values", &self.values)
            .field("saved", &self.saved)
            .field("env", &self.env)
            .field("storage", &self.storage)
            .field("input_devices", &self.input_devices)
            .field("editing", &self.editing)
            .field("draft", &self.draft)
            .field("status", &self.status)
            .finish()
    }
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
        let input_device_id = saved
            .input_device_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env.input_device_id
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
            })
            .map(str::trim)
            .map(ToOwned::to_owned);
        let trace_enabled = saved
            .trace_enabled
            .unwrap_or_else(|| trace_enabled_from_value(env.trace_enabled.as_deref()));
        let realtime_enabled = saved.realtime_enabled.unwrap_or(false);

        Self {
            ollama_endpoint: endpoint,
            planner_model,
            refine_model,
            paraformer_model_dir,
            input_device_id,
            trace_enabled,
            realtime_enabled,
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

    pub(crate) fn paraformer_model_dir(&self) -> Option<&Path> {
        self.paraformer_model_dir.as_deref()
    }

    pub(crate) fn input_device_id(&self) -> Option<&str> {
        self.input_device_id.as_deref()
    }

    pub(crate) fn trace_enabled(&self) -> bool {
        self.trace_enabled
    }

    pub(crate) fn realtime_enabled(&self) -> bool {
        self.realtime_enabled
    }
}

impl EnvVoiceSettings {
    pub(crate) fn from_env() -> Self {
        Self {
            ollama_endpoint: std::env::var("MULTISOUL_OLLAMA_ENDPOINT").ok(),
            planner_model: std::env::var("MULTISOUL_OLLAMA_MODEL").ok(),
            refine_model: std::env::var("MULTISOUL_REFINE_MODEL").ok(),
            paraformer_model_dir: std::env::var("MULTISOUL_PARAFORMER_MODEL_DIR")
                .ok()
                .map(PathBuf::from),
            input_device_id: std::env::var("MULTISOUL_VOICE_INPUT_DEVICE_ID").ok(),
            trace_enabled: std::env::var("MULTISOUL_VOICE_TRACE").ok(),
        }
    }
}

impl SavedVoiceSettings {
    pub(super) fn load(store: &SettingsStore) -> storage::StorageResult<Self> {
        ignore_legacy_openai_realtime_keys(store)?;

        Ok(Self {
            ollama_endpoint: non_empty_option(store.get_string(KEY_OLLAMA_ENDPOINT)?),
            planner_model: non_empty_option(store.get_string(KEY_OLLAMA_MODEL)?),
            refine_model: non_empty_option(store.get_string(KEY_REFINE_MODEL)?),
            paraformer_model_dir: non_empty_option(store.get_string(KEY_PARAFORMER_MODEL_DIR)?)
                .map(PathBuf::from),
            input_device_id: non_empty_option(store.get_string(KEY_INPUT_DEVICE_ID)?),
            trace_enabled: store
                .get_string(KEY_TRACE_ENABLED)?
                .as_deref()
                .and_then(stored_bool),
            realtime_enabled: store
                .get_string(KEY_REALTIME_ENABLED)?
                .as_deref()
                .and_then(stored_bool),
        })
    }
}

impl VoiceSettingsField {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::OllamaEndpoint => 0,
            Self::PlannerModel => 1,
            Self::RefineModel => 2,
        }
    }
}

pub(super) fn non_empty_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn ignore_legacy_openai_realtime_keys(store: &SettingsStore) -> storage::StorageResult<()> {
    let _ = store.get_string(KEY_OPENAI_API_KEY)?;
    let _ = store.get_string(KEY_OPENAI_REALTIME_MODEL)?;
    let _ = store.get_string(KEY_OPENAI_CARD_PLANNER_MODEL)?;
    let _ = store.get_string(KEY_OPENAI_SIDECAR_PATH)?;
    Ok(())
}

fn first_non_empty(saved: Option<&str>, env: Option<&str>, default: &str) -> String {
    saved
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env.filter(|value| !value.trim().is_empty()))
        .map(str::trim)
        .unwrap_or(default)
        .to_string()
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
