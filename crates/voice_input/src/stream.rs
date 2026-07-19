use crate::audio_capture::{
    pcm_stream_chunk_from_samples, AudioCapture, PCM_STREAM_SAMPLE_RATE_HZ,
};
use crate::permissions::ensure_microphone_authorized;
use crate::VoiceInputError;
use crate::{AudioQueueStats, PcmEvent, PCM_QUEUE_CHUNKS};
use audio::PcmAudioChunk;
use crossbeam_channel::bounded;
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

#[cfg(feature = "test-support")]
use std::sync::{Mutex, OnceLock};

const STREAM_CHUNK_QUEUE: usize = 64;

pub struct PcmStreamSession {
    stop_tx: Option<mpsc::Sender<()>>,
    capture_thread: Option<JoinHandle<()>>,
    forward_thread: Option<JoinHandle<()>>,
}

impl PcmStreamSession {
    pub fn cancel(mut self) {
        self.stop_and_join();
    }

    fn stop_and_join(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(capture_thread) = self.capture_thread.take() {
            let _ = capture_thread.join();
        }
        if let Some(forward_thread) = self.forward_thread.take() {
            let _ = forward_thread.join();
        }
    }
}

impl Drop for PcmStreamSession {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

#[cfg(feature = "test-support")]
fn test_pcm_stream() -> &'static Mutex<Option<Vec<PcmAudioChunk>>> {
    static STREAM: OnceLock<Mutex<Option<Vec<PcmAudioChunk>>>> = OnceLock::new();
    STREAM.get_or_init(|| Mutex::new(None))
}

#[cfg(feature = "test-support")]
pub fn install_test_pcm_stream(chunks: Vec<PcmAudioChunk>) {
    *test_pcm_stream()
        .lock()
        .expect("test PCM stream lock must not be poisoned") = Some(chunks);
}

#[cfg(feature = "test-support")]
fn take_test_pcm_stream() -> Option<(PcmStreamSession, async_channel::Receiver<PcmAudioChunk>)> {
    let chunks = test_pcm_stream()
        .lock()
        .expect("test PCM stream lock must not be poisoned")
        .take()?;
    let (sender, receiver) = async_channel::bounded(chunks.len().max(1));
    for chunk in chunks {
        sender
            .send_blocking(chunk)
            .expect("bounded test PCM stream must accept its pre-sized chunks");
    }
    drop(sender);
    Some((
        PcmStreamSession {
            stop_tx: None,
            capture_thread: None,
            forward_thread: None,
        },
        receiver,
    ))
}

pub fn start_pcm_stream(
    input_device_id: Option<String>,
) -> Result<(PcmStreamSession, async_channel::Receiver<PcmAudioChunk>), VoiceInputError> {
    #[cfg(feature = "test-support")]
    if let Some(stream) = take_test_pcm_stream() {
        return Ok(stream);
    }
    tracing::info!(
        target: "norma_voice",
        input_device_selected = input_device_id.is_some(),
        sample_rate_hz = PCM_STREAM_SAMPLE_RATE_HZ,
        "starting microphone PCM capture"
    );
    ensure_microphone_authorized()?;

    let (pcm_tx, pcm_rx) = bounded::<PcmEvent>(PCM_QUEUE_CHUNKS);
    let (chunk_tx, chunk_rx) = async_channel::bounded::<PcmAudioChunk>(STREAM_CHUNK_QUEUE);
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<Result<(), VoiceInputError>>();

    let capture_thread = spawn_capture_thread(pcm_tx, stop_rx, started_tx, input_device_id)?;
    match started_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(
                target: "norma_voice",
                %error,
                "microphone PCM capture failed to start"
            );
            let _ = capture_thread.join();
            return Err(error);
        }
        Err(_) => {
            let _ = capture_thread.join();
            return Err(VoiceInputError::Audio(
                "pcm stream capture startup closed before reporting status".to_string(),
            ));
        }
    }

    let forward_thread = match spawn_forward_thread(pcm_rx, chunk_tx) {
        Ok(thread) => thread,
        Err(error) => {
            let _ = stop_tx.send(());
            let _ = capture_thread.join();
            return Err(error);
        }
    };

    tracing::info!(target: "norma_voice", "microphone PCM capture threads ready");

    Ok((
        PcmStreamSession {
            stop_tx: Some(stop_tx),
            capture_thread: Some(capture_thread),
            forward_thread: Some(forward_thread),
        },
        chunk_rx,
    ))
}

fn spawn_capture_thread(
    pcm_tx: crossbeam_channel::Sender<PcmEvent>,
    stop_rx: mpsc::Receiver<()>,
    started_tx: mpsc::Sender<Result<(), VoiceInputError>>,
    input_device_id: Option<String>,
) -> Result<JoinHandle<()>, VoiceInputError> {
    std::thread::Builder::new()
        .name("voice_input_pcm_capture".to_string())
        .spawn(move || {
            let stats = Arc::new(AudioQueueStats::default());
            let capture = AudioCapture::start(
                pcm_tx,
                stats.clone(),
                input_device_id.as_deref(),
                PCM_STREAM_SAMPLE_RATE_HZ,
            );
            match capture {
                Ok(capture) => {
                    tracing::info!(
                        target: "norma_voice",
                        "AVAudioEngine microphone capture started"
                    );
                    let _ = started_tx.send(Ok(()));
                    let _ = stop_rx.recv();
                    capture.stop();
                    tracing::info!(
                        target: "norma_voice",
                        sent_chunks = stats.sent_chunks(),
                        dropped_chunks = stats.dropped_chunks(),
                        "AVAudioEngine microphone capture stopped"
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        target: "norma_voice",
                        %error,
                        "AVAudioEngine microphone capture initialization failed"
                    );
                    let _ = started_tx.send(Err(error));
                }
            }
        })
        .map_err(|error| {
            VoiceInputError::Audio(format!("could not spawn pcm capture thread: {error}"))
        })
}

fn spawn_forward_thread(
    pcm_rx: crossbeam_channel::Receiver<PcmEvent>,
    chunk_tx: async_channel::Sender<PcmAudioChunk>,
) -> Result<JoinHandle<()>, VoiceInputError> {
    std::thread::Builder::new()
        .name("voice_input_pcm_forwarder".to_string())
        .spawn(move || forward_pcm_events(pcm_rx, chunk_tx))
        .map_err(|error| {
            VoiceInputError::Audio(format!(
                "could not spawn pcm stream forwarder thread: {error}"
            ))
        })
}

fn forward_pcm_events(
    pcm_rx: crossbeam_channel::Receiver<PcmEvent>,
    chunk_tx: async_channel::Sender<PcmAudioChunk>,
) {
    let mut first_chunk_forwarded = false;
    while let Ok(event) = pcm_rx.recv() {
        match event {
            PcmEvent::Samples(samples) => {
                let Some(chunk) = pcm_stream_chunk_from_samples(&samples) else {
                    continue;
                };
                match chunk_tx.try_send(chunk) {
                    Ok(()) => {
                        if !first_chunk_forwarded {
                            first_chunk_forwarded = true;
                            tracing::info!(
                                target: "norma_voice",
                                samples = samples.len(),
                                "first microphone PCM chunk reached realtime pipeline"
                            );
                        }
                    }
                    Err(async_channel::TrySendError::Full(_)) => {}
                    Err(async_channel::TrySendError::Closed(_)) => break,
                }
            }
            PcmEvent::EndOfAudio => break,
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/stream_tests.rs"]
mod stream_tests;
