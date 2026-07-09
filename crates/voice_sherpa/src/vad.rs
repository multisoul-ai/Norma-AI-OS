use crate::samples::{find_vad_model_file, pcm16_chunk_to_f32_samples, resolve_vad_model_dir};
use crate::SpeechError;
use audio::PcmAudioChunk;
use dialogue::{DialogueError, VadClient, VadEvent};
use std::path::Path;

const VAD_BUFFER_SECONDS: f32 = 30.0;

pub struct SherpaVadClient {
    inner: sherpa_onnx::VoiceActivityDetector,
    was_detected: bool,
}

impl SherpaVadClient {
    pub fn from_asr_model_dir(asr_model_dir: &Path) -> Result<Self, SpeechError> {
        let vad_dir = resolve_vad_model_dir(asr_model_dir)?;
        Self::from_vad_model_dir(&vad_dir)
    }

    pub fn from_vad_model_dir(vad_dir: &Path) -> Result<Self, SpeechError> {
        let model_path = find_vad_model_file(vad_dir)?;
        let config = build_vad_config(&model_path);
        let inner = sherpa_onnx::VoiceActivityDetector::create(&config, VAD_BUFFER_SECONDS)
            .ok_or_else(|| {
                SpeechError::Recognizer("failed to create VoiceActivityDetector".into())
            })?;
        Ok(Self {
            inner,
            was_detected: false,
        })
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
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<VadEvent>, DialogueError> {
        let samples = pcm16_chunk_to_f32_samples(chunk).map_err(DialogueError::Message)?;
        self.inner.accept_waveform(&samples);
        let mut events = Vec::new();
        let detected = self.inner.detected();

        if detected && !self.was_detected {
            events.push(VadEvent::SpeechStart);
        }

        while let Some(segment) = self.inner.front() {
            let _ = segment.n();
            self.inner.pop();
            events.push(VadEvent::SpeechEnd);
        }

        if !detected
            && self.was_detected
            && !events.iter().any(|event| *event == VadEvent::SpeechEnd)
        {
            events.push(VadEvent::SpeechEnd);
        }

        self.was_detected = detected;
        Ok(events)
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.inner.clear();
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
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<VadEvent>, DialogueError> {
        let samples = pcm16_chunk_to_f32_samples(chunk).map_err(DialogueError::Message)?;
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
