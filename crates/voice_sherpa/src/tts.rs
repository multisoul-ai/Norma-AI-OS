use crate::audio::pcm16_bytes_from_f32_samples;
use crate::SpeechError;
use audio::{PcmAudioChunk, PcmFormat};
use dialogue::{DialogueError, TtsClient};
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsMatchaModelConfig, OfflineTtsModelConfig,
};
use std::path::{Path, PathBuf};

/// Energy-free silent PCM for unit tests.
pub struct FakeTtsClient {
    sample_rate_hz: u32,
}

impl FakeTtsClient {
    pub fn silent_pcm16_16k() -> Self {
        Self {
            sample_rate_hz: 16_000,
        }
    }

    pub fn silent_pcm16_24k() -> Self {
        Self {
            sample_rate_hz: 24_000,
        }
    }
}

impl TtsClient for FakeTtsClient {
    fn synthesize(&mut self, text: &str) -> Result<PcmAudioChunk, DialogueError> {
        if text.trim().is_empty() {
            return Err(DialogueError::Message("empty TTS text".to_string()));
        }
        let frame_count = (self.sample_rate_hz / 10) as usize;
        let bytes = vec![0u8; frame_count * 2];
        PcmAudioChunk::new(PcmFormat::Pcm16, self.sample_rate_hz, 1, bytes)
            .map_err(|error| DialogueError::Message(format!("invalid fake TTS chunk: {error:?}")))
    }
}

pub struct SherpaTtsClient {
    inner: OfflineTts,
    generation: GenerationConfig,
}

impl SherpaTtsClient {
    pub fn from_kokoro_model_dir(model_dir: &Path) -> Result<Self, SpeechError> {
        if !model_dir.exists() {
            return Err(SpeechError::ModelNotFound(model_dir.display().to_string()));
        }
        let model_path = resolve_kokoro_model_path(model_dir)?;
        let config = OfflineTtsConfig {
            model: OfflineTtsModelConfig {
                kokoro: OfflineTtsKokoroModelConfig {
                    model: Some(path_string(&model_path)),
                    voices: Some(path_string(&model_dir.join("voices.bin"))),
                    tokens: Some(path_string(&model_dir.join("tokens.txt"))),
                    data_dir: Some(path_string(&model_dir.join("espeak-ng-data"))),
                    lexicon: Some(format!(
                        "{},{}",
                        path_string(&model_dir.join("lexicon-us-en.txt")),
                        path_string(&model_dir.join("lexicon-zh.txt"))
                    )),
                    ..Default::default()
                },
                num_threads: 2,
                provider: Some("cpu".to_string()),
                debug: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let inner = OfflineTts::create(&config).ok_or_else(|| {
            SpeechError::Recognizer("failed to create OfflineTts with Kokoro model".to_string())
        })?;
        let generation = generation_config_from_env();
        Ok(Self { inner, generation })
    }

    pub fn from_model_dir(model_dir: &Path, vocoder_path: &Path) -> Result<Self, SpeechError> {
        if !model_dir.exists() {
            return Err(SpeechError::ModelNotFound(model_dir.display().to_string()));
        }
        if !vocoder_path.exists() {
            return Err(SpeechError::ModelNotFound(
                vocoder_path.display().to_string(),
            ));
        }

        let config = OfflineTtsConfig {
            model: OfflineTtsModelConfig {
                matcha: OfflineTtsMatchaModelConfig {
                    acoustic_model: Some(path_string(&model_dir.join("model-steps-3.onnx"))),
                    vocoder: Some(path_string(vocoder_path)),
                    lexicon: Some(path_string(&model_dir.join("lexicon.txt"))),
                    tokens: Some(path_string(&model_dir.join("tokens.txt"))),
                    dict_dir: Some(path_string(&model_dir.join("dict"))),
                    noise_scale: 0.667,
                    length_scale: 1.0,
                    ..Default::default()
                },
                num_threads: 2,
                provider: Some("cpu".to_string()),
                debug: false,
                ..Default::default()
            },
            rule_fsts: Some(format!(
                "{},{},{}",
                path_string(&model_dir.join("phone.fst")),
                path_string(&model_dir.join("date.fst")),
                path_string(&model_dir.join("number.fst"))
            )),
            ..Default::default()
        };
        let inner = OfflineTts::create(&config).ok_or_else(|| {
            SpeechError::Recognizer("failed to create OfflineTts with Matcha model".to_string())
        })?;
        let generation = generation_config_from_env();
        Ok(Self { inner, generation })
    }

    pub fn from_env_or_default() -> Result<Self, SpeechError> {
        Self::from_kokoro_model_dir(&resolve_kokoro_model_dir()?)
    }
}

impl TtsClient for SherpaTtsClient {
    fn synthesize(&mut self, text: &str) -> Result<PcmAudioChunk, DialogueError> {
        if text.trim().is_empty() {
            return Err(DialogueError::Message("empty TTS text".to_string()));
        }
        let audio = self
            .inner
            .generate_with_config(text, &self.generation, None::<fn(&[f32], f32) -> bool>)
            .ok_or_else(|| DialogueError::Message("TTS synthesis failed".to_string()))?;
        let sample_rate_hz = u32::try_from(audio.sample_rate()).map_err(|_| {
            DialogueError::Message(format!(
                "unsupported TTS sample rate: {}",
                audio.sample_rate()
            ))
        })?;
        if !matches!(sample_rate_hz, 16_000 | 22_050 | 24_000) {
            return Err(DialogueError::Message(format!(
                "unsupported TTS sample rate: {sample_rate_hz}"
            )));
        }
        let bytes = pcm16_bytes_from_f32_samples(audio.samples());
        PcmAudioChunk::new(PcmFormat::Pcm16, sample_rate_hz, 1, bytes).map_err(|error| {
            DialogueError::Message(format!("invalid synthesized PCM chunk: {error:?}"))
        })
    }
}

pub fn resolve_kokoro_model_dir() -> Result<PathBuf, SpeechError> {
    if let Ok(path) = std::env::var("MULTISOUL_KOKORO_TTS_MODEL_DIR") {
        let dir = PathBuf::from(path);
        if dir.exists() {
            return Ok(dir);
        }
        return Err(SpeechError::ModelNotFound(dir.display().to_string()));
    }
    let path = repo_root().join("assets/models/tts/kokoro-int8-multi-lang-v1_1");
    if path.exists() {
        return Ok(path);
    }
    Err(SpeechError::ModelNotFound(
        "no Kokoro TTS model found; run scripts/download-kokoro-tts-model.sh".to_string(),
    ))
}

#[cfg(test)]
pub fn resolve_matcha_model_dir() -> Result<PathBuf, SpeechError> {
    if let Ok(path) = std::env::var("MULTISOUL_MATCHA_TTS_MODEL_DIR") {
        let dir = PathBuf::from(path);
        if dir.exists() {
            return Ok(dir);
        }
        return Err(SpeechError::ModelNotFound(dir.display().to_string()));
    }
    let path = repo_root().join("assets/models/tts/matcha-icefall-zh-baker");
    if path.exists() {
        return Ok(path);
    }
    Err(SpeechError::ModelNotFound(
        "no Matcha TTS model found; run scripts/download-matcha-tts-model.sh".to_string(),
    ))
}

#[cfg(test)]
pub fn resolve_matcha_vocoder_path() -> Result<PathBuf, SpeechError> {
    if let Ok(path) = std::env::var("MULTISOUL_MATCHA_TTS_VOCODER") {
        let file = PathBuf::from(path);
        if file.exists() {
            return Ok(file);
        }
        return Err(SpeechError::ModelNotFound(file.display().to_string()));
    }
    let path = repo_root().join("assets/models/tts/vocos-22khz-univ.onnx");
    if path.exists() {
        Ok(path)
    } else {
        Err(SpeechError::ModelNotFound(path.display().to_string()))
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("voice_sherpa crate should live under crates/voice_sherpa")
        .to_path_buf()
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn resolve_kokoro_model_path(model_dir: &Path) -> Result<PathBuf, SpeechError> {
    for filename in ["model.int8.onnx", "model.onnx"] {
        let path = model_dir.join(filename);
        if path.exists() {
            return Ok(path);
        }
    }
    Err(SpeechError::ModelNotFound(format!(
        "no model.int8.onnx or model.onnx found in {}",
        model_dir.display()
    )))
}

fn env_i32(name: &str) -> Option<i32> {
    std::env::var(name).ok()?.parse().ok()
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name).ok()?.parse().ok()
}

fn generation_config_from_env() -> GenerationConfig {
    GenerationConfig {
        sid: env_i32("MULTISOUL_TTS_SPEAKER_ID").unwrap_or(0),
        speed: env_f32("MULTISOUL_TTS_SPEED").unwrap_or(1.0),
        ..Default::default()
    }
}

#[cfg(test)]
#[path = "../tests/unit/tts_tests.rs"]
mod tts_tests;
