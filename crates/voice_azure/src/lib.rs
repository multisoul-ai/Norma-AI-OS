use serde::Deserialize;
use std::time::Duration;

const SHORT_AUDIO_PATH: &str = "/speech/recognition/conversation/cognitiveservices/v1";
const WAV_CONTENT_TYPE: &str = "audio/wav; codecs=audio/pcm; samplerate=16000";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Eq, PartialEq)]
pub struct AzureSpeechConfig {
    endpoint: String,
    subscription_key: String,
    locale: String,
}

impl AzureSpeechConfig {
    pub fn new(
        endpoint: impl Into<String>,
        subscription_key: impl Into<String>,
        locale: impl Into<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            subscription_key: subscription_key.into(),
            locale: locale.into(),
        }
    }

    pub fn for_region(
        region: &str,
        subscription_key: impl Into<String>,
        locale: impl Into<String>,
    ) -> Result<Self, AzureSpeechError> {
        let region = region.trim().to_ascii_lowercase();
        if region.is_empty()
            || !region
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return Err(AzureSpeechError::InvalidConfig(
                "region must contain only ASCII letters, digits, or '-'".to_string(),
            ));
        }
        Ok(Self::new(
            format!("https://{region}.stt.speech.microsoft.com"),
            subscription_key,
            locale,
        ))
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl std::fmt::Debug for AzureSpeechConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AzureSpeechConfig")
            .field("endpoint", &self.endpoint)
            .field("subscription_key", &"[REDACTED]")
            .field("locale", &self.locale)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AzureRecognition {
    pub display_text: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AzureSpeechError {
    #[error("invalid Azure Speech configuration: {0}")]
    InvalidConfig(String),
    #[error("Azure Speech request failed: {0}")]
    Request(String),
    #[error("Azure Speech response could not be decoded: {0}")]
    Decode(String),
    #[error("Azure Speech recognition failed with status {0}")]
    Recognition(String),
    #[error("Azure Speech recognition produced an empty transcript")]
    EmptyTranscript,
}

pub struct AzureSpeechClient {
    config: AzureSpeechConfig,
}

impl AzureSpeechClient {
    pub fn new(config: AzureSpeechConfig) -> Self {
        Self { config }
    }

    pub fn transcribe_short_wav(
        &self,
        wav_bytes: &[u8],
    ) -> Result<AzureRecognition, AzureSpeechError> {
        let endpoint = format!(
            "{}{}?language={}",
            self.config.endpoint.trim_end_matches('/'),
            SHORT_AUDIO_PATH,
            self.config.locale
        );
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .new_agent();
        let mut response = agent
            .post(&endpoint)
            .header("Ocp-Apim-Subscription-Key", &self.config.subscription_key)
            .header("Content-Type", WAV_CONTENT_TYPE)
            .send(wav_bytes)
            .map_err(|error| AzureSpeechError::Request(error.to_string()))?;
        let payload: AzureRecognitionResponse = response
            .body_mut()
            .read_json()
            .map_err(|error| AzureSpeechError::Decode(error.to_string()))?;
        if payload.recognition_status != "Success" {
            return Err(AzureSpeechError::Recognition(payload.recognition_status));
        }
        if payload.display_text.trim().is_empty() {
            return Err(AzureSpeechError::EmptyTranscript);
        }
        Ok(AzureRecognition {
            display_text: payload.display_text,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AzureRecognitionResponse {
    recognition_status: String,
    display_text: String,
}

#[cfg(test)]
#[path = "../tests/unit/client_tests.rs"]
mod client_tests;
