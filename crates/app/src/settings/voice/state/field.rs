#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceSettingsField {
    BaseUrl,
    Model,
    ApiKey,
    AzureRegion,
    AzureLocale,
    AzureApiKey,
    MiniMaxApiKey,
    MiniMaxVoiceId,
    MiniMaxVoiceSearch,
    DoubaoApiKey,
}

impl VoiceSettingsField {
    pub(crate) fn stable_name(self) -> &'static str {
        match self {
            Self::BaseUrl => "agent_base_url",
            Self::Model => "agent_model",
            Self::ApiKey => "agent_api_key",
            Self::AzureRegion => "azure_region",
            Self::AzureLocale => "azure_locale",
            Self::AzureApiKey => "azure_api_key",
            Self::MiniMaxApiKey => "tts_minimax_api_key",
            Self::MiniMaxVoiceId => "tts_minimax_voice_id",
            Self::MiniMaxVoiceSearch => "tts_minimax_voice_search",
            Self::DoubaoApiKey => "doubao_api_key",
        }
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Self::BaseUrl => 0,
            Self::Model => 1,
            Self::ApiKey => 2,
            Self::AzureRegion => 3,
            Self::AzureLocale => 4,
            Self::AzureApiKey => 5,
            Self::MiniMaxApiKey => 6,
            Self::MiniMaxVoiceId => 7,
            Self::MiniMaxVoiceSearch => 8,
            Self::DoubaoApiKey => 9,
        }
    }
}
