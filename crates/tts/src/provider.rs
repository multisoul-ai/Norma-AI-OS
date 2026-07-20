use crate::{
    builtin_model_catalog, ProviderVoiceCatalog, TtsClient, TtsError, TtsErrorKind, TtsModelId,
    TtsVoiceId,
};
use serde::{Deserialize, Serialize};

pub const LOCAL_TTS_PROVIDER_ID: &str = "local";
pub const MINIMAX_TTS_PROVIDER_ID: &str = "minimax";
pub const MINIMAX_SPEECH_TURBO_MODEL_ID: &str = "speech-2.8-turbo";
pub const MINIMAX_SPEECH_HD_MODEL_ID: &str = "speech-2.8-hd";

/// Stable identity for one built-in TTS provider.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TtsProviderId(String);

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TtsProviderIdError {
    #[error("TTS provider id cannot be blank")]
    Blank,
    #[error("TTS provider id contains unsupported characters")]
    InvalidCharacters,
}

impl TtsProviderId {
    pub fn parse(value: impl Into<String>) -> Result<Self, TtsProviderIdError> {
        let value = value.into();
        let value = value.trim();
        if value.is_empty() {
            return Err(TtsProviderIdError::Blank);
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(TtsProviderIdError::InvalidCharacters);
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsProviderModelDescriptor {
    id: TtsModelId,
    display_name: String,
    recommended: bool,
}

impl TtsProviderModelDescriptor {
    fn new(id: &str, display_name: &str, recommended: bool) -> Self {
        Self {
            id: TtsModelId::parse(id).expect("built-in TTS model IDs must be non-blank"),
            display_name: display_name.to_string(),
            recommended,
        }
    }

    pub fn id(&self) -> &TtsModelId {
        &self.id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn recommended(&self) -> bool {
        self.recommended
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsProviderDescriptor {
    id: TtsProviderId,
    display_name: String,
    remote: bool,
    models: Vec<TtsProviderModelDescriptor>,
    configuration: TtsProviderConfiguration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtsProviderConfiguration {
    OnDeviceModel,
    ApiKeyWithVoiceCatalog,
}

impl TtsProviderDescriptor {
    pub fn id(&self) -> &TtsProviderId {
        &self.id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn is_remote(&self) -> bool {
        self.remote
    }

    pub fn models(&self) -> &[TtsProviderModelDescriptor] {
        &self.models
    }

    pub fn configuration(&self) -> TtsProviderConfiguration {
        self.configuration
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuiltinTtsProviderRegistry {
    providers: Vec<TtsProviderDescriptor>,
}

impl BuiltinTtsProviderRegistry {
    pub fn providers(&self) -> &[TtsProviderDescriptor] {
        &self.providers
    }

    pub fn provider(&self, id: &str) -> Option<&TtsProviderDescriptor> {
        self.providers
            .iter()
            .find(|provider| provider.id().as_str() == id)
    }
}

/// Returns the closed, compiled-in Provider registry shipped with this App version.
pub fn builtin_provider_registry() -> BuiltinTtsProviderRegistry {
    let local_models = builtin_model_catalog()
        .entries()
        .iter()
        .map(|model| TtsProviderModelDescriptor {
            id: model.id().clone(),
            display_name: model.display_name().to_string(),
            recommended: model.recommended(),
        })
        .collect();
    BuiltinTtsProviderRegistry {
        providers: vec![
            TtsProviderDescriptor {
                id: TtsProviderId::parse(LOCAL_TTS_PROVIDER_ID)
                    .expect("local built-in Provider ID must be valid"),
                display_name: "On-device".to_string(),
                remote: false,
                models: local_models,
                configuration: TtsProviderConfiguration::OnDeviceModel,
            },
            TtsProviderDescriptor {
                id: TtsProviderId::parse(MINIMAX_TTS_PROVIDER_ID)
                    .expect("MiniMax built-in Provider ID must be valid"),
                display_name: "MiniMax".to_string(),
                remote: true,
                configuration: TtsProviderConfiguration::ApiKeyWithVoiceCatalog,
                models: vec![
                    TtsProviderModelDescriptor::new(
                        MINIMAX_SPEECH_TURBO_MODEL_ID,
                        "Speech 2.8 Turbo",
                        true,
                    ),
                    TtsProviderModelDescriptor::new(
                        MINIMAX_SPEECH_HD_MODEL_ID,
                        "Speech 2.8 HD",
                        false,
                    ),
                ],
            },
        ],
    }
}

/// Provider-neutral construction input for a compiled-in remote TTS client.
pub struct BuiltinRemoteTtsRequest {
    pub provider_id: TtsProviderId,
    pub model_id: TtsModelId,
    pub voice_id: TtsVoiceId,
    pub credential: String,
}

/// Constructs a reviewed, compiled-in remote provider without exposing vendor protocol types.
pub fn create_builtin_remote_tts_client(
    request: BuiltinRemoteTtsRequest,
) -> Result<Box<dyn TtsClient>, TtsError> {
    match request.provider_id.as_str() {
        MINIMAX_TTS_PROVIDER_ID => {
            let config = crate::providers::minimax::MiniMaxTtsConfig::new(
                request.model_id,
                request.voice_id,
            )?;
            let provider = crate::providers::minimax::MiniMaxProvider::new(request.credential)?;
            Ok(Box::new(provider.tts_client(config)))
        }
        _ => Err(TtsError::new(
            TtsErrorKind::Configuration,
            "unsupported built-in remote TTS Provider",
        )),
    }
}

/// Queries the account Voice catalog through the same closed provider registry boundary.
pub fn query_builtin_provider_voices(
    provider_id: &TtsProviderId,
    credential: String,
) -> Result<ProviderVoiceCatalog, TtsError> {
    match provider_id.as_str() {
        MINIMAX_TTS_PROVIDER_ID => {
            crate::providers::minimax::MiniMaxProvider::new(credential)?.query_voices()
        }
        _ => Err(TtsError::new(
            TtsErrorKind::Configuration,
            "selected built-in TTS Provider does not expose a remote Voice catalog",
        )),
    }
}
