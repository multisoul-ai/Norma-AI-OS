use crate::{
    AsrError, AsrProviderCapabilities, RecognitionCancellation, Recognizer, RecognizerFactory,
};
use audio::PcmAudioChunk;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(feature = "test-support")]
mod test_support;
#[cfg(feature = "test-support")]
pub use test_support::{
    install_azure_speech_endpoint_override_for_tests, AzureSpeechEndpointOverrideGuard,
};

const SHORT_AUDIO_PATH: &str = "/speech/recognition/conversation/cognitiveservices/v1";
const WAV_CONTENT_TYPE: &str = "audio/wav; codecs=audio/pcm; samplerate=16000";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

// Azure Speech-to-text locales supported by the public short-audio endpoint. The catalog is
// versioned with the app so Settings never accepts an arbitrary locale string.
const AZURE_SPEECH_LOCALES: &[&str] = &[
    "af-ZA",
    "am-ET",
    "ar-AE",
    "ar-BH",
    "ar-DZ",
    "ar-EG",
    "ar-IL",
    "ar-IQ",
    "ar-JO",
    "ar-KW",
    "ar-LB",
    "ar-LY",
    "ar-MA",
    "ar-OM",
    "ar-PS",
    "ar-QA",
    "ar-SA",
    "ar-SY",
    "ar-TN",
    "ar-YE",
    "as-IN",
    "az-AZ",
    "bg-BG",
    "bho-IN",
    "bn-IN",
    "bs-BA",
    "ca-ES",
    "cs-CZ",
    "cy-GB",
    "da-DK",
    "de-AT",
    "de-CH",
    "de-DE",
    "el-GR",
    "en-AU",
    "en-CA",
    "en-GB",
    "en-GH",
    "en-HK",
    "en-IE",
    "en-IN",
    "en-KE",
    "en-NG",
    "en-NZ",
    "en-PH",
    "en-SG",
    "en-TZ",
    "en-US",
    "en-ZA",
    "es-AR",
    "es-BO",
    "es-CL",
    "es-CO",
    "es-CR",
    "es-CU",
    "es-DO",
    "es-EC",
    "es-ES",
    "es-GQ",
    "es-GT",
    "es-HN",
    "es-MX",
    "es-NI",
    "es-PA",
    "es-PE",
    "es-PR",
    "es-PY",
    "es-SV",
    "es-US",
    "es-UY",
    "es-VE",
    "et-EE",
    "eu-ES",
    "fa-IR",
    "fi-FI",
    "fil-PH",
    "fr-BE",
    "fr-CA",
    "fr-CH",
    "fr-FR",
    "ga-IE",
    "gl-ES",
    "gu-IN",
    "he-IL",
    "hi-IN",
    "hr-HR",
    "hu-HU",
    "hy-AM",
    "id-ID",
    "is-IS",
    "it-CH",
    "it-IT",
    "ja-JP",
    "jv-ID",
    "ka-GE",
    "kk-KZ",
    "km-KH",
    "kn-IN",
    "ko-KR",
    "lo-LA",
    "lt-LT",
    "lv-LV",
    "mk-MK",
    "ml-IN",
    "mn-MN",
    "mr-IN",
    "ms-MY",
    "mt-MT",
    "my-MM",
    "nb-NO",
    "ne-NP",
    "nl-BE",
    "nl-NL",
    "or-IN",
    "pa-IN",
    "pl-PL",
    "ps-AF",
    "pt-BR",
    "pt-PT",
    "ro-RO",
    "ru-RU",
    "si-LK",
    "sk-SK",
    "sl-SI",
    "so-SO",
    "sq-AL",
    "sr-ME",
    "sr-RS",
    "sr-XK",
    "sv-SE",
    "sw-KE",
    "sw-TZ",
    "ta-IN",
    "te-IN",
    "th-TH",
    "tr-TR",
    "uk-UA",
    "ur-IN",
    "uz-UZ",
    "vi-VN",
    "wuu-CN",
    "yue-CN",
    "zh-CN",
    "zh-CN-shandong",
    "zh-CN-sichuan",
    "zh-HK",
    "zh-TW",
    "zu-ZA",
];

pub fn azure_speech_locales() -> &'static [&'static str] {
    AZURE_SPEECH_LOCALES
}

pub fn validate_azure_speech_locale(locale: &str) -> Result<(), AsrError> {
    AZURE_SPEECH_LOCALES
        .contains(&locale.trim())
        .then_some(())
        .ok_or_else(|| AsrError::Provider("unsupported Azure Speech locale".to_string()))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AzureSpeechProfile {
    pub region: String,
    pub locale: String,
    pub credential_ref: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct AzureSpeechSecret(String);

impl AzureSpeechSecret {
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for AzureSpeechSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AzureSpeechSecret([REDACTED])")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct AzureSpeechConfig {
    region: String,
    locale: String,
    secret: AzureSpeechSecret,
    endpoint_override: Option<String>,
}

impl AzureSpeechConfig {
    pub fn new(
        region: impl Into<String>,
        locale: impl Into<String>,
        secret: AzureSpeechSecret,
    ) -> Result<Self, AsrError> {
        let region = region.into().trim().to_ascii_lowercase();
        if region.is_empty()
            || !region
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return Err(AsrError::Provider("invalid Azure region".to_string()));
        }
        let locale = locale.into().trim().to_string();
        validate_azure_speech_locale(&locale)?;
        if secret.expose().trim().is_empty() {
            return Err(AsrError::Provider("missing Azure credential".to_string()));
        }
        Ok(Self {
            region,
            locale,
            secret,
            endpoint_override: None,
        })
    }

    pub fn stt_endpoint(&self) -> String {
        #[cfg(feature = "test-support")]
        if let Some(endpoint) = test_support::endpoint_override()
            .lock()
            .expect("Azure test endpoint override lock must not be poisoned")
            .clone()
        {
            return endpoint;
        }
        self.endpoint_override
            .clone()
            .unwrap_or_else(|| format!("https://{}.stt.speech.microsoft.com", self.region))
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    #[cfg(test)]
    pub(crate) fn for_test_endpoint(
        endpoint: String,
        locale: impl Into<String>,
        secret: AzureSpeechSecret,
    ) -> Self {
        Self {
            region: "test".to_string(),
            locale: locale.into(),
            secret,
            endpoint_override: Some(endpoint),
        }
    }
}

impl std::fmt::Debug for AzureSpeechConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AzureSpeechConfig")
            .field("region", &self.region)
            .field("locale", &self.locale)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone)]
pub struct AzureSpeechClient {
    config: AzureSpeechConfig,
    cancellation: RecognitionCancellation,
}

impl AzureSpeechClient {
    pub fn new(config: AzureSpeechConfig) -> Self {
        Self {
            config,
            cancellation: RecognitionCancellation::new(),
        }
    }

    pub fn with_cancellation(mut self, cancellation: RecognitionCancellation) -> Self {
        self.cancellation = cancellation;
        self
    }

    pub fn transcribe_short_wav(&self, wav_bytes: &[u8]) -> Result<Option<String>, AsrError> {
        let payload = self.send_short_wav(wav_bytes, false)?;
        if payload.recognition_status != "Success" {
            return Err(AsrError::Provider(format!(
                "Azure recognition status: {}",
                payload.recognition_status
            )));
        }
        let text = payload.display_text.trim();
        Ok((!text.is_empty()).then(|| text.to_string()))
    }

    pub fn validate_credentials(&self) -> Result<(), AsrError> {
        // 100 ms × 16,000 samples/s × 2 bytes/sample = 3,200 bytes of silence.
        let silence = vec![0_u8; 3_200];
        let wav = pcm16_mono_wav(&silence, 16_000)?;
        let _ = self.send_short_wav(&wav, true)?;
        Ok(())
    }

    fn send_short_wav(
        &self,
        wav_bytes: &[u8],
        validation: bool,
    ) -> Result<AzureRecognitionResponse, AsrError> {
        let validation_query = if validation {
            "&initialSilenceTimeoutMs=1000"
        } else {
            ""
        };
        let endpoint = format!(
            "{}{}?language={}{}",
            self.config.stt_endpoint().trim_end_matches('/'),
            SHORT_AUDIO_PATH,
            self.config.locale(),
            validation_query
        );
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| AsrError::Provider(format!("Azure client failed: {error}")))?;
        let request = client
            .post(endpoint)
            .header("Ocp-Apim-Subscription-Key", self.config.secret.expose())
            .header("Content-Type", WAV_CONTENT_TYPE)
            .body(wav_bytes.to_vec());
        let cancellation = self.cancellation.clone();
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|error| AsrError::Provider(format!("Azure runtime failed: {error}")))?;
        runtime.block_on(async move {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    Err(AsrError::Provider("Azure request cancelled".to_string()))
                }
                response = request.send() => {
                    let response = response.map_err(|error| {
                        AsrError::Provider(format!("Azure request failed: {error}"))
                    })?;
                    let response = response.error_for_status().map_err(|error| {
                        AsrError::Provider(format!("Azure request failed: {error}"))
                    })?;
                    response.json().await.map_err(|error| {
                        AsrError::Provider(format!("Azure response decode failed: {error}"))
                    })
                }
            }
        })
    }
}

pub struct AzureSpeechRecognizer {
    client: AzureSpeechClient,
    pcm_bytes: Vec<u8>,
}

impl AzureSpeechRecognizer {
    pub fn new(client: AzureSpeechClient) -> Self {
        Self {
            client,
            pcm_bytes: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.pcm_bytes.clear();
    }
}

impl Recognizer for AzureSpeechRecognizer {
    fn accept_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<String>, AsrError> {
        if chunk.sample_rate_hz != 16_000
            || chunk.channels != 1
            || !chunk.bytes.len().is_multiple_of(2)
        {
            return Err(AsrError::InvalidPcm);
        }
        self.pcm_bytes.extend_from_slice(&chunk.bytes);
        Ok(Vec::new())
    }

    fn finish(&mut self) -> Result<Option<String>, AsrError> {
        if self.pcm_bytes.is_empty() {
            return Ok(None);
        }
        let wav = pcm16_mono_wav(&self.pcm_bytes, 16_000)?;
        self.client.transcribe_short_wav(&wav)
    }

    fn set_cancellation_token(&mut self, cancellation: RecognitionCancellation) {
        self.client = self.client.clone().with_cancellation(cancellation);
    }
}

#[derive(Clone)]
pub struct AzureSpeechRecognizerFactory {
    config: AzureSpeechConfig,
}

impl AzureSpeechRecognizerFactory {
    pub fn new(config: AzureSpeechConfig) -> Self {
        Self { config }
    }
}

impl RecognizerFactory for AzureSpeechRecognizerFactory {
    fn capabilities(&self) -> AsrProviderCapabilities {
        AsrProviderCapabilities::new(false, Some(60))
    }

    fn create(&self) -> Result<Box<dyn Recognizer>, AsrError> {
        Ok(Box::new(AzureSpeechRecognizer::new(
            AzureSpeechClient::new(self.config.clone()),
        )))
    }
}

fn pcm16_mono_wav(pcm: &[u8], sample_rate_hz: u32) -> Result<Vec<u8>, AsrError> {
    let data_len = u32::try_from(pcm.len())
        .map_err(|_| AsrError::Provider("PCM is too large for WAV".to_string()))?;
    let riff_len = 36_u32
        .checked_add(data_len)
        .ok_or_else(|| AsrError::Provider("WAV length overflow".to_string()))?;
    let byte_rate = sample_rate_hz * 2;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_len.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate_hz.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(pcm);
    Ok(wav)
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AzureRecognitionResponse {
    recognition_status: String,
    display_text: String,
}

#[cfg(test)]
#[path = "../../../tests/unit/azure_tests.rs"]
mod azure_tests;
