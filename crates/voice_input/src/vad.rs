use crate::samples::pcm16_chunk_to_f32_samples;
use crate::samples::{find_vad_model_file, resolve_vad_model_dir};
use crate::VoiceInputError;
use crate::{VadClient, VadEvent};
use audio::PcmAudioChunk;
use std::path::Path;

const VAD_BUFFER_SECONDS: f32 = 30.0;

/// A detector-confirmed segment, in mono 16kHz samples since the last reset.
/// Forced maximum-duration segments may belong to the same ongoing utterance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpeechSpan {
    pub start_sample: u64,
    pub end_sample: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VadObservation {
    pub events: Vec<VadEvent>,
    pub completed_spans: Vec<SpeechSpan>,
    pub speech_active: bool,
}

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

impl SherpaVadClient {
    pub fn observe_pcm(
        &mut self,
        chunk: &PcmAudioChunk,
    ) -> Result<VadObservation, VoiceInputError> {
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

                let mut completed_spans = Vec::new();
                while let Some(segment) = inner.front() {
                    let start = u64::try_from(segment.start()).map_err(|_| {
                        VoiceInputError::Recognizer("negative VAD segment start".into())
                    })?;
                    let length = u64::try_from(segment.n()).map_err(|_| {
                        VoiceInputError::Recognizer("negative VAD segment length".into())
                    })?;
                    completed_spans.push(SpeechSpan {
                        start_sample: start,
                        end_sample: start + length,
                    });
                    inner.pop();
                    events.push(VadEvent::SpeechEnd);
                }

                if !detected && self.was_detected && !events.contains(&VadEvent::SpeechEnd) {
                    events.push(VadEvent::SpeechEnd);
                }

                self.was_detected = detected;
                Ok(VadObservation {
                    events,
                    completed_spans,
                    speech_active: detected,
                })
            }
            #[cfg(feature = "test-support")]
            SherpaVadBackend::Deterministic(fake) => fake.observe_pcm(chunk),
        }
    }
}

impl VadClient for SherpaVadClient {
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<VadEvent>, VoiceInputError> {
        self.observe_pcm(chunk).map(|observed| observed.events)
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
    samples_seen: u64,
    active_start: Option<u64>,
}

impl FakeEnergyVadClient {
    pub fn new(threshold: f32) -> Self {
        Self {
            threshold,
            was_detected: false,
            samples_seen: 0,
            active_start: None,
        }
    }
}

impl FakeEnergyVadClient {
    fn observe_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<VadObservation, VoiceInputError> {
        let samples = pcm16_chunk_to_f32_samples(chunk).map_err(VoiceInputError::Recognizer)?;
        let energy =
            samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32;
        let detected = energy >= self.threshold;
        let mut events = Vec::new();
        let mut completed_spans = Vec::new();
        if detected && !self.was_detected {
            self.active_start = Some(self.samples_seen);
            events.push(VadEvent::SpeechStart);
        }
        if !detected && self.was_detected {
            if let Some(start_sample) = self.active_start.take() {
                completed_spans.push(SpeechSpan {
                    start_sample,
                    end_sample: self.samples_seen,
                });
            }
            events.push(VadEvent::SpeechEnd);
        }
        self.was_detected = detected;
        self.samples_seen += samples.len() as u64;
        Ok(VadObservation {
            events,
            completed_spans,
            speech_active: detected,
        })
    }
}

impl VadClient for FakeEnergyVadClient {
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<VadEvent>, VoiceInputError> {
        self.observe_pcm(chunk).map(|observed| observed.events)
    }

    fn reset(&mut self) {
        self.samples_seen = 0;
        self.active_start = None;
        self.was_detected = false;
    }
}

#[cfg(test)]
#[path = "../tests/unit/vad_tests.rs"]
mod vad_tests;
