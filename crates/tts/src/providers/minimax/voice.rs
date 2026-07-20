use super::protocol::provider_status_error;
use super::{MiniMaxTtsClient, MiniMaxTtsConfig};
use crate::{
    ProviderVoiceCatalog, ProviderVoiceDescriptor, TtsError, TtsErrorKind, TtsVoiceId,
    TtsVoiceOrigin,
};
use serde::Deserialize;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

const MINIMAX_GET_VOICE_URL: &str = "https://api.minimaxi.com/v1/get_voice";
const VOICE_CATALOG_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) trait MiniMaxVoiceApi: Send + Sync {
    fn query_all(&self, api_key: &str) -> Result<String, TtsError>;
}

struct ProductionMiniMaxVoiceApi;

impl MiniMaxVoiceApi for ProductionMiniMaxVoiceApi {
    fn query_all(&self, api_key: &str) -> Result<String, TtsError> {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            // 15 seconds bounds connect + response so a stalled catalog cannot retain a worker.
            .timeout_global(Some(VOICE_CATALOG_TIMEOUT))
            .build()
            .new_agent();
        let response = agent
            .post(MINIMAX_GET_VOICE_URL)
            .header("Authorization", &format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .send_json(serde_json::json!({ "voice_type": "all" }))
            .map_err(|_| {
                TtsError::new(
                    TtsErrorKind::Transport,
                    "MiniMax Voice catalog request failed",
                )
                .retryable_before_acceptance()
            })?;
        let status = response.status().as_u16();
        if matches!(status, 401 | 403) {
            return Err(TtsError::new(
                TtsErrorKind::Authentication,
                "MiniMax rejected the supplied credential",
            ));
        }
        if status == 429 {
            return Err(TtsError::new(
                TtsErrorKind::RateLimit,
                "MiniMax Voice catalog rate limit was reached",
            ));
        }
        if !(200..300).contains(&status) {
            return Err(TtsError::new(
                TtsErrorKind::Protocol,
                format!("MiniMax Voice catalog returned HTTP {status}"),
            ));
        }
        response.into_body().read_to_string().map_err(|_| {
            TtsError::new(
                TtsErrorKind::Transport,
                "could not read MiniMax Voice catalog response",
            )
        })
    }
}

pub struct MiniMaxProvider {
    api_key: String,
    voice_api: Arc<dyn MiniMaxVoiceApi>,
}

impl MiniMaxProvider {
    pub fn new(api_key: String) -> Result<Self, TtsError> {
        Self::new_with_voice_api(api_key, Arc::new(ProductionMiniMaxVoiceApi))
    }

    pub(super) fn new_with_voice_api(
        api_key: String,
        voice_api: Arc<dyn MiniMaxVoiceApi>,
    ) -> Result<Self, TtsError> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(TtsError::new(
                TtsErrorKind::Authentication,
                "MiniMax API Key cannot be blank",
            ));
        }
        Ok(Self { api_key, voice_api })
    }

    pub fn query_voices(&self) -> Result<ProviderVoiceCatalog, TtsError> {
        normalize_voice_catalog(&self.voice_api.query_all(&self.api_key)?)
    }

    pub fn tts_client(&self, config: MiniMaxTtsConfig) -> MiniMaxTtsClient {
        MiniMaxTtsClient::new(self.api_key.clone(), config)
    }
}

impl fmt::Debug for MiniMaxProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MiniMaxProvider")
            .field("credential_configured", &true)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct VoiceCatalogResponse {
    #[serde(default)]
    system_voice: Vec<VoiceResponse>,
    #[serde(default)]
    voice_cloning: Vec<VoiceResponse>,
    #[serde(default)]
    voice_generation: Vec<VoiceResponse>,
    base_resp: Option<VoiceBaseResponse>,
}

#[derive(Deserialize)]
struct VoiceResponse {
    voice_id: String,
    voice_name: Option<String>,
}

#[derive(Deserialize)]
struct VoiceBaseResponse {
    status_code: i64,
}

fn normalize_voice_catalog(body: &str) -> Result<ProviderVoiceCatalog, TtsError> {
    let response: VoiceCatalogResponse = serde_json::from_str(body).map_err(|_| {
        TtsError::new(
            TtsErrorKind::Protocol,
            "MiniMax returned an invalid Voice catalog",
        )
    })?;
    if let Some(base) = response.base_resp {
        if base.status_code != 0 {
            return Err(provider_status_error(base.status_code));
        }
    }
    let mut voices = Vec::with_capacity(
        response.system_voice.len()
            + response.voice_cloning.len()
            + response.voice_generation.len(),
    );
    append_voices(&mut voices, response.system_voice, TtsVoiceOrigin::System)?;
    append_voices(&mut voices, response.voice_cloning, TtsVoiceOrigin::Cloned)?;
    append_voices(
        &mut voices,
        response.voice_generation,
        TtsVoiceOrigin::Generated,
    )?;
    Ok(ProviderVoiceCatalog::new(voices))
}

fn append_voices(
    output: &mut Vec<ProviderVoiceDescriptor>,
    voices: Vec<VoiceResponse>,
    origin: TtsVoiceOrigin,
) -> Result<(), TtsError> {
    for voice in voices {
        let voice_id = TtsVoiceId::parse(voice.voice_id).map_err(|_| {
            TtsError::new(
                TtsErrorKind::Protocol,
                "MiniMax Voice catalog contained a blank Voice ID",
            )
        })?;
        let display_name = voice
            .voice_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| voice_id.as_str().to_string());
        output.push(ProviderVoiceDescriptor::new(voice_id, display_name, origin));
    }
    Ok(())
}
