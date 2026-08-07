use crate::samples::pcm16_chunk_to_f32_samples;
use crate::samples::{find_vad_model_file, resolve_vad_model_dir};
use crate::VoiceInputError;
use crate::{VadClient, VadEvent};
use audio::PcmAudioChunk;
use std::path::Path;

const VAD_BUFFER_SECONDS: f32 = 30.0;

pub struct SherpaVadClient {
    inner: SherpaVadBackend,
    was_detected: bool,
}

enum SherpaVadBackend {
    Sherpa(sherpa_onnx::VoiceActivityDetector),
    #[cfg(feature = "test-support")]
    Deterministic(FakeEnergyVadClient),
}

impl SherpaVadClient {
    pub fn from_asr_model_dir(asr_model_dir: &Path) -> Result<Self, VoiceInputError> {
        let vad_dir = resolve_vad_model_dir(asr_model_dir)?;
        Self::from_vad_model_dir(&vad_dir)
    }

    pub fn from_vad_model_dir(vad_dir: &Path) -> Result<Self, VoiceInputError> {
        let model_path = find_vad_model_file(vad_dir)?;
        let config = build_vad_config(&model_path);
        let inner = sherpa_onnx::VoiceActivityDetector::create(&config, VAD_BUFFER_SECONDS)
            .ok_or_else(|| {
                VoiceInputError::Recognizer("failed to create VoiceActivityDetector".into())
            })?;
        Ok(Self {
            inner: SherpaVadBackend::Sherpa(inner),
            was_detected: false,
        })
    }

    #[cfg(feature = "test-support")]
    pub fn deterministic_for_tests() -> Self {
        Self {
            inner: SherpaVadBackend::Deterministic(FakeEnergyVadClient::new(0.01)),
            was_detected: false,
        }
    }
}

fn build_vad_config(model_path: &Path) -> sherpa_onnx::VadModelConfig {
    sherpa_onnx::VadModelConfig {
        silero_vad: sherpa_onnx::SileroVadModelConfig {
            model: Some(model_path.to_string_lossy().into_owned()),
            threshold: 0.5,
            min_silence_duration: 0.25,
            min_speech_duration: 0.15,
            window_size: 512,
            max_speech_duration: 20.0,
        },
        sample_rate: 16_000,
        num_threads: 1,
        provider: Some("cpu".to_string()),
        debug: false,
        ..Default::default()
    }
}

impl VadClient for SherpaVadClient {
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<VadEvent>, VoiceInputError> {
        match &mut self.inner {
            SherpaVadBackend::Sherpa(inner) => {
                let samples =
                    pcm16_chunk_to_f32_samples(chunk).map_err(VoiceInputError::Recognizer)?;
                inner.accept_waveform(&samples);
                let mut events = Vec::new();
                let detected = inner.detected();

                if detected && !self.was_detected {
                    events.push(VadEvent::SpeechStart);
                }

                while let Some(segment) = inner.front() {
                    let _ = segment.n();
                    inner.pop();
                    events.push(VadEvent::SpeechEnd);
                }

                if !detected && self.was_detected && !events.contains(&VadEvent::SpeechEnd) {
                    events.push(VadEvent::SpeechEnd);
                }

                self.was_detected = detected;
                Ok(events)
            }
            #[cfg(feature = "test-support")]
            SherpaVadBackend::Deterministic(fake) => fake.push_pcm(chunk),
        }
    }

    fn reset(&mut self) {
        match &mut self.inner {
            SherpaVadBackend::Sherpa(inner) => {
                inner.reset();
                inner.clear();
            }
            #[cfg(feature = "test-support")]
            SherpaVadBackend::Deterministic(fake) => fake.reset(),
        }
        self.was_detected = false;
    }
}

/// Energy-threshold VAD for unit tests without model assets.
pub struct FakeEnergyVadClient {
    threshold: f32,
    was_detected: bool,
}

impl FakeEnergyVadClient {
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,
            was_detected: false,
        }
    }
}

impl VadClient for FakeEnergyVadClient {
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<VadEvent>, VoiceInputError> {
        let samples = pcm16_chunk_to_f32_samples(chunk).map_err(VoiceInputError::Recognizer)?;
        let energy =
            samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32;
        let detected = energy >= self.threshold;
        let mut events = Vec::new();
        if detected && !self.was_detected {
            events.push(VadEvent::SpeechStart);
        }
        if !detected && self.was_detected {
            events.push(VadEvent::SpeechEnd);
        }
        self.was_detected = detected;
        Ok(events)
    }

    fn reset(&mut self) {
        self.was_detected = false;
    }
}

#[cfg(test)]
#[path = "../tests/unit/vad_tests.rs"]
mod vad_tests;
