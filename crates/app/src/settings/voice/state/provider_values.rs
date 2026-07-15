use std::fmt;

pub(crate) const DEFAULT_OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434";
pub(crate) const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub(super) const KEY_AGENT_PROVIDER: &str = "agent.provider.active";
pub(super) const KEY_AGENT_OLLAMA_BASE_URL: &str = "agent.provider.ollama.base_url";
pub(super) const KEY_AGENT_OLLAMA_API_KEY: &str = "agent.provider.ollama.api_key";
pub(super) const KEY_AGENT_OLLAMA_MODEL: &str = "agent.provider.ollama.model";
pub(super) const KEY_AGENT_OPENAI_BASE_URL: &str = "agent.provider.openai.base_url";
pub(super) const KEY_AGENT_OPENAI_API_KEY: &str = "agent.provider.openai.api_key";
pub(super) const KEY_AGENT_OPENAI_MODEL: &str = "agent.provider.openai.model";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AgentProviderKind {
    #[default]
    Ollama,
    OpenAiCompatible,
}

impl AgentProviderKind {
    pub(crate) fn from_stored(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ollama" => Some(Self::Ollama),
            "openai" | "openai_compatible" => Some(Self::OpenAiCompatible),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::OpenAiCompatible => "openai",
        }
    }
}

#[derive(Clone, Default, Eq, PartialEq)]
pub(crate) struct SavedProviderProfile {
    pub(crate) base_url: Option<String>,
    pub(crate) api_key: Option<String>,
    pub(crate) model: Option<String>,
}

impl fmt::Debug for SavedProviderProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SavedProviderProfile")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("model", &self.model)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ProviderProfileValues {
    pub(super) base_url: String,
    pub(super) api_key: Option<String>,
    pub(super) model: String,
}

impl fmt::Debug for ProviderProfileValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderProfileValues")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("model", &self.model)
            .finish()
    }
}

impl ProviderProfileValues {
    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    pub(crate) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    pub(crate) fn model(&self) -> &str {
        &self.model
    }
}
