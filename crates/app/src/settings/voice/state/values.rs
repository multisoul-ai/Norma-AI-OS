use super::provider_values::{
    AgentProviderKind, ProviderProfileValues, SavedProviderProfile, DEFAULT_OLLAMA_ENDPOINT,
    DEFAULT_OPENAI_BASE_URL, KEY_AGENT_OLLAMA_API_KEY, KEY_AGENT_OLLAMA_BASE_URL,
    KEY_AGENT_OLLAMA_MODEL, KEY_AGENT_OPENAI_API_KEY, KEY_AGENT_OPENAI_BASE_URL,
    KEY_AGENT_OPENAI_MODEL, KEY_AGENT_PROVIDER,
};
use super::VoiceSettingsState;
use std::fmt;
use std::path::{Path, PathBuf};
use storage::SettingsStore;
use voice::OllamaVoicePlanner;

pub(super) const KEY_OLLAMA_ENDPOINT: &str = "voice.ollama.endpoint";
pub(super) const KEY_OLLAMA_MODEL: &str = "voice.ollama.model";
pub(super) const KEY_DIALOGUE_LLM_MODE: &str = "voice.dialogue.llm_mode";
pub(super) const KEY_DIALOGUE_MODEL: &str = "voice.dialogue.model";
pub(super) const KEY_REFINE_MODEL: &str = "voice.refine.model";
pub(super) const KEY_PARAFORMER_MODEL_DIR: &str = "voice.paraformer.model_dir";
pub(super) const KEY_INPUT_DEVICE_ID: &str = "voice.input_device_id";
pub(super) const KEY_TRACE_ENABLED: &str = "voice.trace_enabled";
pub(super) const KEY_REALTIME_ENABLED: &str = "voice.realtime.enabled";
pub(super) const KEY_OPENAI_API_KEY: &str = "voice.openai.api_key";
pub(super) const KEY_OPENAI_REALTIME_MODEL: &str = "voice.openai.realtime_model";
pub(super) const KEY_OPENAI_CARD_PLANNER_MODEL: &str = "voice.openai.card_planner_model";
pub(super) const KEY_OPENAI_SIDECAR_PATH: &str = "voice.openai.sidecar_path";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DialogueLlmMode {
    Mock,
    Ollama,
}

impl DialogueLlmMode {
    pub(crate) fn from_stored(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "mock" => Some(Self::Mock),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Ollama => "ollama",
        }
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub(crate) struct SavedVoiceSettings {
    pub(crate) agent_provider: Option<AgentProviderKind>,
    pub(crate) ollama_profile: SavedProviderProfile,
    pub(crate) openai_profile: SavedProviderProfile,
    pub(crate) ollama_endpoint: Option<String>,
    pub(crate) planner_model: Option<String>,
    pub(crate) dialogue_llm_mode: Option<DialogueLlmMode>,
    pub(crate) dialogue_model: Option<String>,
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
    pub(crate) dialogue_model: Option<String>,
    pub(crate) refine_model: Option<String>,
    pub(crate) paraformer_model_dir: Option<PathBuf>,
    pub(crate) input_device_id: Option<String>,
    pub(crate) trace_enabled: Option<String>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct VoiceSettingsValues {
    pub(crate) agent_provider: AgentProviderKind,
    pub(crate) ollama_profile: ProviderProfileValues,
    pub(crate) openai_profile: ProviderProfileValues,
    pub(crate) ollama_endpoint: String,
    pub(crate) planner_model: String,
    pub(crate) dialogue_llm_mode: DialogueLlmMode,
    pub(crate) dialogue_model: String,
    pub(crate) refine_model: String,
    pub(crate) paraformer_model_dir: Option<PathBuf>,
    pub(crate) input_device_id: Option<String>,
    pub(crate) trace_enabled: bool,
    pub(crate) realtime_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceSettingsField {
    AgentBaseUrl,
    AgentModel,
    AgentApiKey,
    OllamaEndpoint,
    PlannerModel,
    DialogueModel,
    RefineModel,
}

impl VoiceSettingsField {
    pub(crate) fn stable_name(self) -> &'static str {
        match self {
            Self::AgentBaseUrl => "agent_base_url",
            Self::AgentModel => "agent_model",
            Self::AgentApiKey => "agent_api_key",
            Self::OllamaEndpoint => "ollama_endpoint",
            Self::PlannerModel => "planner_model",
            Self::DialogueModel => "dialogue_model",
            Self::RefineModel => "refine_model",
        }
    }
}

impl fmt::Debug for SavedVoiceSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedVoiceSettings")
            .field("agent_provider", &self.agent_provider)
            .field("ollama_profile", &self.ollama_profile)
            .field("openai_profile_base_url", &self.openai_profile.base_url)
            .field("openai_profile_model", &self.openai_profile.model)
            .field(
                "openai_profile_api_key",
                &self.openai_profile.api_key.as_ref().map(|_| "<redacted>"),
            )
            .field("ollama_endpoint", &self.ollama_endpoint)
            .field("planner_model", &self.planner_model)
            .field("dialogue_llm_mode", &self.dialogue_llm_mode)
            .field("dialogue_model", &self.dialogue_model)
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
            .field("agent_provider", &self.agent_provider)
            .field("ollama_profile", &self.ollama_profile)
            .field("openai_profile", &self.openai_profile)
            .field("ollama_endpoint", &self.ollama_endpoint)
            .field("planner_model", &self.planner_model)
            .field("dialogue_llm_mode", &self.dialogue_llm_mode)
            .field("dialogue_model", &self.dialogue_model)
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
        let draft = if self.editing == Some(VoiceSettingsField::AgentApiKey) {
            "<redacted>"
        } else {
            self.draft.as_str()
        };
        f.debug_struct("VoiceSettingsState")
            .field("values", &self.values)
            .field("saved", &self.saved)
            .field("env", &self.env)
            .field("storage", &self.storage)
            .field("input_devices", &self.input_devices)
            .field("editing", &self.editing)
            .field("draft", &draft)
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
        let dialogue_llm_mode = saved.dialogue_llm_mode.unwrap_or(DialogueLlmMode::Mock);
        let dialogue_model = first_non_empty(
            saved.dialogue_model.as_deref(),
            env.dialogue_model.as_deref(),
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

        let agent_provider = saved.agent_provider.unwrap_or_default();
        let ollama_profile = ProviderProfileValues {
            base_url: first_non_empty(
                saved
                    .ollama_profile
                    .base_url
                    .as_deref()
                    .or(saved.ollama_endpoint.as_deref()),
                env.ollama_endpoint.as_deref(),
                DEFAULT_OLLAMA_ENDPOINT,
            ),
            api_key: non_empty_option(saved.ollama_profile.api_key.clone()),
            model: first_non_empty(
                saved
                    .ollama_profile
                    .model
                    .as_deref()
                    .or(saved.dialogue_model.as_deref()),
                env.dialogue_model.as_deref(),
                OllamaVoicePlanner::default_model(),
            ),
        };
        let openai_profile = ProviderProfileValues {
            base_url: first_non_empty(
                saved.openai_profile.base_url.as_deref(),
                None,
                DEFAULT_OPENAI_BASE_URL,
            ),
            api_key: non_empty_option(saved.openai_profile.api_key.clone()),
            model: saved
                .openai_profile
                .model
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .to_string(),
        };

        Self {
            agent_provider,
            ollama_profile,
            openai_profile,
            ollama_endpoint: endpoint,
            planner_model,
            dialogue_llm_mode,
            dialogue_model,
            refine_model,
            paraformer_model_dir,
            input_device_id,
            trace_enabled,
            realtime_enabled,
        }
    }

    pub(crate) fn agent_provider(&self) -> AgentProviderKind {
        self.agent_provider
    }

    pub(crate) fn active_provider_profile(&self) -> &ProviderProfileValues {
        match self.agent_provider {
            AgentProviderKind::Ollama => &self.ollama_profile,
            AgentProviderKind::OpenAiCompatible => &self.openai_profile,
        }
    }

    pub(crate) fn agent_provider_config(
        &self,
    ) -> Result<agent::ProviderConfig, agent::ProviderError> {
        let profile = self.active_provider_profile();
        let protocol = match self.agent_provider {
            AgentProviderKind::Ollama => agent::ProviderProtocol::Ollama,
            AgentProviderKind::OpenAiCompatible => agent::ProviderProtocol::OpenAiCompatible,
        };
        let config = agent::ProviderConfig::new(
            protocol,
            profile.base_url.clone(),
            profile.model.clone(),
            profile.api_key.clone(),
        );
        config.validate()?;
        Ok(config)
    }

    pub(crate) fn ollama_endpoint(&self) -> &str {
        &self.ollama_endpoint
    }

    pub(crate) fn planner_model(&self) -> &str {
        &self.planner_model
    }

    pub(crate) fn dialogue_llm_mode(&self) -> DialogueLlmMode {
        self.dialogue_llm_mode
    }

    pub(crate) fn dialogue_model(&self) -> &str {
        &self.dialogue_model
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
            dialogue_model: std::env::var("MULTISOUL_DIALOGUE_MODEL").ok(),
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
        let legacy_ollama_endpoint = non_empty_option(store.get_string(KEY_OLLAMA_ENDPOINT)?);
        let legacy_openai_api_key = non_empty_option(store.get_string(KEY_OPENAI_API_KEY)?);
        let legacy_openai_model =
            non_empty_option(store.get_string(KEY_OPENAI_CARD_PLANNER_MODEL)?);
        Ok(Self {
            agent_provider: store
                .get_string(KEY_AGENT_PROVIDER)?
                .as_deref()
                .and_then(AgentProviderKind::from_stored),
            ollama_profile: SavedProviderProfile {
                base_url: non_empty_option(store.get_string(KEY_AGENT_OLLAMA_BASE_URL)?)
                    .or_else(|| legacy_ollama_endpoint.clone()),
                api_key: non_empty_option(store.get_string(KEY_AGENT_OLLAMA_API_KEY)?),
                model: non_empty_option(store.get_string(KEY_AGENT_OLLAMA_MODEL)?),
            },
            openai_profile: SavedProviderProfile {
                base_url: non_empty_option(store.get_string(KEY_AGENT_OPENAI_BASE_URL)?),
                api_key: non_empty_option(store.get_string(KEY_AGENT_OPENAI_API_KEY)?)
                    .or(legacy_openai_api_key),
                model: non_empty_option(store.get_string(KEY_AGENT_OPENAI_MODEL)?)
                    .or(legacy_openai_model),
            },
            ollama_endpoint: legacy_ollama_endpoint,
            planner_model: non_empty_option(store.get_string(KEY_OLLAMA_MODEL)?),
            dialogue_llm_mode: store
                .get_string(KEY_DIALOGUE_LLM_MODE)?
                .as_deref()
                .and_then(DialogueLlmMode::from_stored),
            dialogue_model: non_empty_option(store.get_string(KEY_DIALOGUE_MODEL)?),
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
            Self::AgentBaseUrl => 0,
            Self::AgentModel => 1,
            Self::AgentApiKey => 2,
            Self::OllamaEndpoint => 3,
            Self::PlannerModel => 4,
            Self::DialogueModel => 5,
            Self::RefineModel => 6,
        }
    }
}

pub(super) fn non_empty_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
