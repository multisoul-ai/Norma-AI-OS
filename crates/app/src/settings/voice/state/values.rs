use super::provider_values::{
    AgentProviderKind, ProviderProfileValues, SavedProviderProfile, DEFAULT_OLLAMA_ENDPOINT,
    DEFAULT_OPENAI_BASE_URL, KEY_AGENT_OLLAMA_API_KEY, KEY_AGENT_OLLAMA_BASE_URL,
    KEY_AGENT_OLLAMA_MODEL, KEY_AGENT_OPENAI_API_KEY, KEY_AGENT_OPENAI_BASE_URL,
    KEY_AGENT_OPENAI_MODEL, KEY_AGENT_PROVIDER,
};
use asr::AsrModelId;
use storage::SettingsStore;
use tts::{TtsModelId, TtsVoiceId};
use voice::OllamaVoicePlanner;
mod debug;
pub(super) const KEY_OLLAMA_ENDPOINT: &str = "voice.ollama.endpoint";
pub(super) const KEY_OLLAMA_MODEL: &str = "voice.ollama.model";
pub(super) const KEY_DIALOGUE_LLM_MODE: &str = "voice.dialogue.llm_mode";
pub(super) const KEY_DIALOGUE_MODEL: &str = "voice.dialogue.model";
pub(super) const KEY_REFINE_MODEL: &str = "voice.refine.model";
pub(super) const KEY_INTERACTION_MODE: &str = "voice.interaction.mode";
pub(super) const KEY_SINGLE_VOICE_COMMAND_TRIGGER: &str = "voice.single_voice_command.trigger";
pub(super) const LEGACY_KEY_DICTATION_TRIGGER: &str = "voice.dictation.trigger";
pub(super) const KEY_ASR_MODEL_ID: &str = "voice.asr.model_id";
pub(super) const KEY_TTS_MODEL_ID: &str = "voice.tts.model_id";
pub(super) const KEY_TTS_VOICE_ID: &str = "voice.tts.voice_id";
pub(super) const KEY_INPUT_DEVICE_ID: &str = "voice.input_device_id";
pub(super) const KEY_TRACE_ENABLED: &str = "voice.trace_enabled";
pub(super) const KEY_AGENT_TASK_ANNOUNCEMENTS_ENABLED: &str =
    "voice.agent_task_announcements_enabled";
pub(super) const KEY_VOICE_WINDOW_INSET_ENABLED: &str = "voice.window_inset_enabled";
pub(super) const KEY_OPENAI_API_KEY: &str = "voice.openai.api_key";
pub(super) const KEY_OPENAI_CARD_PLANNER_MODEL: &str = "voice.openai.card_planner_model";
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DialogueLlmMode {
    Mock,
    Ollama,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum VoiceInteractionMode {
    #[default]
    SingleVoiceCommand,
    ContinuousListening,
}
impl VoiceInteractionMode {
    pub(crate) fn from_stored(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dictation" | "single_voice_command" => Some(Self::SingleVoiceCommand),
            "continuous_listening" => Some(Self::ContinuousListening),
            _ => None,
        }
    }
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SingleVoiceCommand => "single_voice_command",
            Self::ContinuousListening => "continuous_listening",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DictationTrigger {
    #[default]
    Hold,
    Toggle,
}
impl DictationTrigger {
    pub(crate) fn from_stored(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "hold" => Some(Self::Hold),
            "toggle" => Some(Self::Toggle),
            _ => None,
        }
    }
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Toggle => "toggle",
        }
    }
}
impl DialogueLlmMode {
    pub(crate) fn from_stored(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "mock" => Some(Self::Mock),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }
}
#[derive(Clone, Default, Eq, PartialEq)]
pub(crate) struct SavedVoiceSettings {
    pub(crate) interaction_mode: Option<VoiceInteractionMode>,
    pub(crate) dictation_trigger: Option<DictationTrigger>,
    pub(crate) asr_model_id: Option<AsrModelId>,
    pub(crate) tts_model_id: Option<TtsModelId>,
    pub(crate) tts_voice_id: Option<TtsVoiceId>,
    pub(crate) agent_provider: Option<AgentProviderKind>,
    pub(crate) ollama_profile: SavedProviderProfile,
    pub(crate) openai_profile: SavedProviderProfile,
    pub(crate) ollama_endpoint: Option<String>,
    pub(crate) planner_model: Option<String>,
    pub(crate) dialogue_llm_mode: Option<DialogueLlmMode>,
    pub(crate) dialogue_model: Option<String>,
    pub(crate) refine_model: Option<String>,
    pub(crate) input_device_id: Option<String>,
    pub(crate) trace_enabled: Option<bool>,
    pub(crate) agent_task_announcements_enabled: Option<bool>,
    pub(crate) voice_window_inset_enabled: Option<bool>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct EnvVoiceSettings {
    pub(crate) ollama_endpoint: Option<String>,
    pub(crate) planner_model: Option<String>,
    pub(crate) dialogue_model: Option<String>,
    pub(crate) refine_model: Option<String>,
    pub(crate) input_device_id: Option<String>,
    pub(crate) trace_enabled: Option<String>,
}
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct VoiceSettingsValues {
    pub(crate) interaction_mode: VoiceInteractionMode,
    pub(crate) dictation_trigger: DictationTrigger,
    pub(crate) asr_model_id: Option<AsrModelId>,
    pub(crate) tts_model_id: Option<TtsModelId>,
    pub(crate) tts_voice_id: Option<TtsVoiceId>,
    pub(crate) agent_provider: AgentProviderKind,
    pub(crate) ollama_profile: ProviderProfileValues,
    pub(crate) openai_profile: ProviderProfileValues,
    pub(crate) ollama_endpoint: String,
    pub(crate) planner_model: String,
    pub(crate) dialogue_llm_mode: DialogueLlmMode,
    pub(crate) dialogue_model: String,
    pub(crate) refine_model: String,
    pub(crate) input_device_id: Option<String>,
    pub(crate) trace_enabled: bool,
    pub(crate) agent_task_announcements_enabled: bool,
    pub(crate) voice_window_inset_enabled: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceSettingsField {
    AgentBaseUrl,
    AgentModel,
    AgentApiKey,
}

impl VoiceSettingsField {
    pub(crate) fn stable_name(self) -> &'static str {
        match self {
            Self::AgentBaseUrl => "agent_base_url",
            Self::AgentModel => "agent_model",
            Self::AgentApiKey => "agent_api_key",
        }
    }
}

impl VoiceSettingsValues {
    pub(crate) fn resolve(saved: &SavedVoiceSettings, env: &EnvVoiceSettings) -> Self {
        let interaction_mode = saved.interaction_mode.unwrap_or_default();
        let dictation_trigger = saved.dictation_trigger.unwrap_or_default();
        let asr_model_id = saved.asr_model_id.clone();
        let tts_model_id = saved.tts_model_id.clone();
        let tts_voice_id = saved.tts_voice_id.clone();
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
        let agent_task_announcements_enabled =
            saved.agent_task_announcements_enabled.unwrap_or(true);
        let voice_window_inset_enabled = saved.voice_window_inset_enabled.unwrap_or(true);

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
            interaction_mode,
            dictation_trigger,
            asr_model_id,
            tts_model_id,
            tts_voice_id,
            agent_provider,
            ollama_profile,
            openai_profile,
            ollama_endpoint: endpoint,
            planner_model,
            dialogue_llm_mode,
            dialogue_model,
            refine_model,
            input_device_id,
            trace_enabled,
            agent_task_announcements_enabled,
            voice_window_inset_enabled,
        }
    }

    pub(crate) fn agent_provider(&self) -> AgentProviderKind {
        self.agent_provider
    }

    pub(crate) fn interaction_mode(&self) -> VoiceInteractionMode {
        self.interaction_mode
    }

    pub(crate) fn dictation_trigger(&self) -> DictationTrigger {
        self.dictation_trigger
    }

    pub(crate) fn asr_model_id(&self) -> Option<&AsrModelId> {
        self.asr_model_id.as_ref()
    }

    pub(crate) fn tts_model_id(&self) -> Option<&TtsModelId> {
        self.tts_model_id.as_ref()
    }
    pub(crate) fn tts_voice_id(&self) -> Option<&TtsVoiceId> {
        self.tts_voice_id.as_ref()
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

    pub(crate) fn input_device_id(&self) -> Option<&str> {
        self.input_device_id.as_deref()
    }

    pub(crate) fn trace_enabled(&self) -> bool {
        self.trace_enabled
    }

    pub(crate) fn agent_task_announcements_enabled(&self) -> bool {
        self.agent_task_announcements_enabled
    }

    pub(crate) fn voice_window_inset_enabled(&self) -> bool {
        self.voice_window_inset_enabled
    }
}
impl EnvVoiceSettings {
    pub(crate) fn from_env() -> Self {
        Self {
            ollama_endpoint: std::env::var("MULTISOUL_OLLAMA_ENDPOINT").ok(),
            planner_model: std::env::var("MULTISOUL_OLLAMA_MODEL").ok(),
            dialogue_model: std::env::var("MULTISOUL_DIALOGUE_MODEL").ok(),
            refine_model: std::env::var("MULTISOUL_REFINE_MODEL").ok(),
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
            interaction_mode: store
                .get_string(KEY_INTERACTION_MODE)?
                .as_deref()
                .and_then(VoiceInteractionMode::from_stored),
            dictation_trigger: store
                .get_string(KEY_SINGLE_VOICE_COMMAND_TRIGGER)?
                .or(store.get_string(LEGACY_KEY_DICTATION_TRIGGER)?)
                .as_deref()
                .and_then(DictationTrigger::from_stored),
            asr_model_id: non_empty_option(store.get_string(KEY_ASR_MODEL_ID)?)
                .and_then(|value| AsrModelId::parse(value).ok()),
            tts_model_id: non_empty_option(store.get_string(KEY_TTS_MODEL_ID)?)
                .and_then(|value| TtsModelId::parse(value).ok()),
            tts_voice_id: non_empty_option(store.get_string(KEY_TTS_VOICE_ID)?)
                .and_then(|value| TtsVoiceId::parse(value).ok()),
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
            input_device_id: non_empty_option(store.get_string(KEY_INPUT_DEVICE_ID)?),
            trace_enabled: store
                .get_string(KEY_TRACE_ENABLED)?
                .as_deref()
                .and_then(stored_bool),
            agent_task_announcements_enabled: store
                .get_string(KEY_AGENT_TASK_ANNOUNCEMENTS_ENABLED)?
                .as_deref()
                .and_then(stored_bool),
            voice_window_inset_enabled: store
                .get_string(KEY_VOICE_WINDOW_INSET_ENABLED)?
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
