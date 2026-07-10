use crate::audio::{pcm_stream_chunk_from_samples, AudioCapture, PCM_STREAM_SAMPLE_RATE_HZ};
use crate::permissions::ensure_microphone_authorized;
use crate::session::{AudioQueueStats, PcmEvent, PCM_QUEUE_CHUNKS};
use crate::SpeechError;
use audio::PcmAudioChunk;
use crossbeam_channel::bounded;
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

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

pub(crate) fn start(
    input_device_id: Option<String>,
) -> Result<(PcmStreamSession, async_channel::Receiver<PcmAudioChunk>), SpeechError> {
    ensure_microphone_authorized()?;

    let (pcm_tx, pcm_rx) = bounded::<PcmEvent>(PCM_QUEUE_CHUNKS);
    let (chunk_tx, chunk_rx) = async_channel::bounded::<PcmAudioChunk>(STREAM_CHUNK_QUEUE);
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<Result<(), SpeechError>>();

    let capture_thread = spawn_capture_thread(pcm_tx, stop_rx, started_tx, input_device_id)?;
    match started_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = capture_thread.join();
            return Err(error);
        }
        Err(_) => {
            let _ = capture_thread.join();
            return Err(SpeechError::Audio(
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
    started_tx: mpsc::Sender<Result<(), SpeechError>>,
    input_device_id: Option<String>,
) -> Result<JoinHandle<()>, SpeechError> {
    std::thread::Builder::new()
        .name("voice_sherpa_pcm_capture".to_string())
        .spawn(move || {
            let stats = Arc::new(AudioQueueStats::default());
            let capture = AudioCapture::start(
                pcm_tx,
                stats,
                input_device_id.as_deref(),
                PCM_STREAM_SAMPLE_RATE_HZ,
            );
            match capture {
                Ok(capture) => {
                    let _ = started_tx.send(Ok(()));
                    let _ = stop_rx.recv();
                    capture.stop();
                }
                Err(error) => {
                    let _ = started_tx.send(Err(error));
                }
            }
        })
        .map_err(|error| SpeechError::Audio(format!("could not spawn pcm capture thread: {error}")))
}

fn spawn_forward_thread(
    pcm_rx: crossbeam_channel::Receiver<PcmEvent>,
    chunk_tx: async_channel::Sender<PcmAudioChunk>,
) -> Result<JoinHandle<()>, SpeechError> {
    std::thread::Builder::new()
        .name("voice_sherpa_pcm_forwarder".to_string())
        .spawn(move || forward_pcm_events(pcm_rx, chunk_tx))
        .map_err(|error| {
            SpeechError::Audio(format!(
                "could not spawn pcm stream forwarder thread: {error}"
            ))
        })
}

fn forward_pcm_events(
    pcm_rx: crossbeam_channel::Receiver<PcmEvent>,
    chunk_tx: async_channel::Sender<PcmAudioChunk>,
) {
    while let Ok(event) = pcm_rx.recv() {
        match event {
            PcmEvent::Samples(samples) => {
                let Some(chunk) = pcm_stream_chunk_from_samples(&samples) else {
                    continue;
                };
                match chunk_tx.try_send(chunk) {
                    Ok(()) => {}
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
