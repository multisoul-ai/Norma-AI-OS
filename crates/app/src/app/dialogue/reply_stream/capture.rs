use audio::{PcmAudioChunk, PcmFormat};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const CAPTURE_DIR_ENV: &str = "NORMA_TTS_CAPTURE_DIR";

#[derive(Default)]
pub(crate) struct TtsPcmCapture {
    sender: Option<async_channel::Sender<PcmAudioChunk>>,
}

impl TtsPcmCapture {
    pub(super) fn start(reply_generation: u64) -> Self {
        let Some(directory) = std::env::var_os(CAPTURE_DIR_ENV) else {
            return Self::default();
        };
        let path = capture_path(PathBuf::from(directory), reply_generation);
        let (sender, receiver) = async_channel::unbounded();
        match std::thread::Builder::new()
            .name("tts_pcm_capture".to_string())
            .spawn(move || capture_worker(receiver, path))
        {
            Ok(_) => Self {
                sender: Some(sender),
            },
            Err(error) => {
                tracing::warn!(
                    target: "norma_voice",
                    %error,
                    "[TTS-TRACE] could not start PCM capture worker"
                );
                Self::default()
            }
        }
    }

    pub(super) fn append(&self, chunk: &PcmAudioChunk) {
        if let Some(sender) = self.sender.as_ref() {
            if sender.try_send(chunk.clone()).is_err() {
                tracing::warn!(
                    target: "norma_voice",
                    "[TTS-TRACE] PCM capture worker is unavailable; capture is incomplete"
                );
            }
        }
    }

    pub(super) fn finish(&mut self) {
        self.sender.take();
    }
}

pub(crate) fn write_pcm_chunks_to_wav(path: &Path, chunks: &[PcmAudioChunk]) -> Result<(), String> {
    let first = chunks
        .first()
        .ok_or_else(|| "cannot write a WAV capture with no PCM chunks".to_string())?;
    if first.format != PcmFormat::Pcm16 {
        return Err("PCM capture only supports Pcm16 chunks".to_string());
    }
    let spec = hound::WavSpec {
        channels: first.channels,
        sample_rate: first.sample_rate_hz,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut writer = hound::WavWriter::create(path, spec).map_err(|error| error.to_string())?;
    for chunk in chunks {
        if chunk.format != first.format
            || chunk.sample_rate_hz != first.sample_rate_hz
            || chunk.channels != first.channels
        {
            return Err("PCM capture chunks have inconsistent formats".to_string());
        }
        for bytes in chunk.bytes.chunks_exact(2) {
            writer
                .write_sample(i16::from_le_bytes([bytes[0], bytes[1]]))
                .map_err(|error| error.to_string())?;
        }
    }
    writer.finalize().map_err(|error| error.to_string())
}

fn capture_path(directory: PathBuf, reply_generation: u64) -> PathBuf {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    directory.join(format!("tts-reply-{reply_generation}-{timestamp_ms}.wav"))
}

fn capture_worker(receiver: async_channel::Receiver<PcmAudioChunk>, path: PathBuf) {
    let mut chunks = Vec::new();
    while let Ok(chunk) = receiver.recv_blocking() {
        chunks.push(chunk);
    }
    if chunks.is_empty() {
        return;
    }
    match write_pcm_chunks_to_wav(&path, &chunks) {
        Ok(()) => tracing::info!(
            target: "norma_voice",
            path = %path.display(),
            chunk_count = chunks.len(),
            "[TTS-TRACE] playback-input WAV capture written"
        ),
        Err(error) => tracing::warn!(
            target: "norma_voice",
            %error,
            path = %path.display(),
            "[TTS-TRACE] could not write playback-input WAV capture"
        ),
    }
}
