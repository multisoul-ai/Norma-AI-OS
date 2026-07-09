use crate::samples::pcm16_chunk_to_f32_samples;
use crate::session::find_model_file;
use crate::SpeechError;
use audio::PcmAudioChunk;
use dialogue::{AsrClient, AsrEvent, DialogueError};
use std::path::Path;

pub struct SherpaAsrStreamClient {
    recognizer: sherpa_onnx::OnlineRecognizer,
    stream: sherpa_onnx::OnlineStream,
    last_partial: String,
}

impl SherpaAsrStreamClient {
    pub fn from_model_dir(model_dir: &Path) -> Result<Self, SpeechError> {
        let config = build_stream_recognizer_config(model_dir)?;
        let recognizer = sherpa_onnx::OnlineRecognizer::create(&config)
            .ok_or_else(|| SpeechError::Recognizer("failed to create OnlineRecognizer".into()))?;
        let stream = recognizer.create_stream();
        Ok(Self {
            recognizer,
            stream,
            last_partial: String::new(),
        })
    }
}

fn build_stream_recognizer_config(
    model_dir: &Path,
) -> Result<sherpa_onnx::OnlineRecognizerConfig, SpeechError> {
    use sherpa_onnx::{OnlineModelConfig, OnlineParaformerModelConfig, OnlineRecognizerConfig};

    let encoder = find_model_file(model_dir, &["encoder.int8.onnx", "encoder.onnx"])?;
    let decoder = find_model_file(model_dir, &["decoder.int8.onnx", "decoder.onnx"])?;
    let tokens = model_dir.join("tokens.txt");
    if !tokens.exists() {
        return Err(SpeechError::ModelNotFound(tokens.display().to_string()));
    }

    Ok(OnlineRecognizerConfig {
        model_config: OnlineModelConfig {
            paraformer: OnlineParaformerModelConfig {
                encoder: Some(encoder.to_string_lossy().into_owned()),
                decoder: Some(decoder.to_string_lossy().into_owned()),
            },
            tokens: Some(tokens.to_string_lossy().into_owned()),
            num_threads: 2,
            provider: Some("cpu".to_string()),
            debug: false,
            ..Default::default()
        },
        ..Default::default()
    })
}

impl AsrClient for SherpaAsrStreamClient {
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<AsrEvent>, DialogueError> {
        let samples = pcm16_chunk_to_f32_samples(chunk).map_err(DialogueError::Message)?;
        self.stream.accept_waveform(16_000, &samples);
        let mut events = Vec::new();
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
            let text = self
                .recognizer
                .get_result(&self.stream)
                .map(|result| result.text.trim().to_string())
                .unwrap_or_default();
            if !text.is_empty() && text != self.last_partial {
                self.last_partial = text.clone();
                events.push(AsrEvent::Partial(text));
            }
        }
        Ok(events)
    }

    fn end_utterance(&mut self) -> Result<Option<String>, DialogueError> {
        self.stream.input_finished();
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
        }
        let text = self
            .recognizer
            .get_result(&self.stream)
            .map(|result| result.text.trim().to_string())
            .unwrap_or_default();
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(text))
        }
    }

    fn reset(&mut self) {
        self.stream = self.recognizer.create_stream();
        self.last_partial.clear();
    }
}

#[cfg(test)]
#[path = "../tests/unit/asr_stream_tests.rs"]
mod asr_stream_tests;
