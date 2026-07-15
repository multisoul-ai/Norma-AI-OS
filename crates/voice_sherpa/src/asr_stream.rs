use crate::recognizer_config::build_online_recognizer_config;
use crate::samples::pcm16_chunk_to_f32_samples;
use crate::SpeechError;
use audio::PcmAudioChunk;
use dialogue::{AsrClient, AsrEvent, DialogueError};

pub struct SherpaAsrStreamClient {
    recognizer: sherpa_onnx::OnlineRecognizer,
    stream: sherpa_onnx::OnlineStream,
    last_partial: String,
}

impl SherpaAsrStreamClient {
    pub fn from_available_model(model: &asr::AvailableAsrModel) -> Result<Self, SpeechError> {
        let config = build_online_recognizer_config(model)?;
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
