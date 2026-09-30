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
use std::time::{Duration, Instant};

#[cfg(feature = "test-support")]
use std::sync::{Mutex, OnceLock};

const STREAM_CHUNK_QUEUE: usize = 64;
// 双工队列最多保留 8 个 chunk，网络暂慢时丢弃旧音频；实际 chunk 时长由设备决定。
const DUPLEX_STREAM_CHUNK_QUEUE: usize = 8;
// 首块 PCM 通常约 100 ms 到达；给 8 个回调周期（800 ms）验证 AEC 采集就绪。
const DUPLEX_CAPTURE_READY_TIMEOUT: Duration = Duration::from_millis(800);

pub struct PcmStreamSession {
    stop_tx: Option<mpsc::Sender<()>>,
    capture_thread: Option<JoinHandle<()>>,
    forward_thread: Option<JoinHandle<()>>,
    #[cfg(feature = "test-support")]
    test_hold_tx: Option<async_channel::Sender<PcmAudioChunk>>,
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
        #[cfg(feature = "test-support")]
        self.test_hold_tx.take();
    }
}

impl Drop for PcmStreamSession {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

#[cfg(feature = "test-support")]
fn test_pcm_stream() -> &'static Mutex<Option<(Vec<PcmAudioChunk>, bool)>> {
    static STREAM: OnceLock<Mutex<Option<(Vec<PcmAudioChunk>, bool)>>> = OnceLock::new();
    STREAM.get_or_init(|| Mutex::new(None))
}

#[cfg(feature = "test-support")]
pub fn install_test_pcm_stream(chunks: Vec<PcmAudioChunk>) {
    *test_pcm_stream()
        .lock()
        .expect("test PCM stream lock must not be poisoned") = Some((chunks, false));
}

#[cfg(feature = "test-support")]
pub fn install_test_duplex_pcm_stream(chunks: Vec<PcmAudioChunk>) {
    *test_pcm_stream()
        .lock()
        .expect("test PCM stream lock must not be poisoned") = Some((chunks, true));
}

#[cfg(feature = "test-support")]
fn take_test_pcm_stream() -> Option<(PcmStreamSession, async_channel::Receiver<PcmAudioChunk>)> {
    let (chunks, keep_open) = test_pcm_stream()
        .lock()
        .expect("test PCM stream lock must not be poisoned")
        .take()?;
    let (sender, receiver) = async_channel::bounded(chunks.len().max(1));
    for chunk in chunks {
        sender
            .send_blocking(chunk)
            .expect("bounded test PCM stream must accept its pre-sized chunks");
    }
    let test_hold_tx = keep_open.then_some(sender);
    Some((
        PcmStreamSession {
            stop_tx: None,
            capture_thread: None,
            forward_thread: None,
            test_hold_tx,
        },
        receiver,
    ))
}

pub fn start_pcm_stream(
    input_device_id: Option<String>,
) -> Result<(PcmStreamSession, async_channel::Receiver<PcmAudioChunk>), VoiceInputError> {
    start_pcm_stream_with_voice_processing(input_device_id, false)
}

pub fn start_duplex_pcm_stream(
    input_device_id: Option<String>,
) -> Result<(PcmStreamSession, async_channel::Receiver<PcmAudioChunk>), VoiceInputError> {
    start_pcm_stream_with_voice_processing(input_device_id, true)
}

fn start_pcm_stream_with_voice_processing(
    input_device_id: Option<String>,
    voice_processing: bool,
) -> Result<(PcmStreamSession, async_channel::Receiver<PcmAudioChunk>), VoiceInputError> {
    #[cfg(feature = "test-support")]
    if let Some(stream) = take_test_pcm_stream() {
        return Ok(stream);
    }
    tracing::info!(
        target: "norma_voice",
        input_device_selected = input_device_id.is_some(),
        sample_rate_hz = PCM_STREAM_SAMPLE_RATE_HZ,
        voice_processing,
        "starting microphone PCM capture"
    );
    ensure_microphone_authorized()?;

    let (pcm_tx, pcm_rx) = bounded::<PcmEvent>(PCM_QUEUE_CHUNKS);
    let queue_capacity = if voice_processing {
        DUPLEX_STREAM_CHUNK_QUEUE
    } else {
        STREAM_CHUNK_QUEUE
    };
    let (chunk_tx, chunk_rx) = async_channel::bounded::<PcmAudioChunk>(queue_capacity);
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<Result<(), VoiceInputError>>();

    let stats = Arc::new(AudioQueueStats::default());
    let capture_thread = spawn_capture_thread(
        pcm_tx,
        stop_rx,
        started_tx,
        input_device_id,
        voice_processing,
        Arc::clone(&stats),
    )?;
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

    let stale_rx = voice_processing.then(|| chunk_rx.clone());
    let forward_thread = match spawn_forward_thread(pcm_rx, chunk_tx, stale_rx) {
        Ok(thread) => thread,
        Err(error) => {
            let _ = stop_tx.send(());
            let _ = capture_thread.join();
            return Err(error);
        }
    };

    tracing::info!(target: "norma_voice", "microphone PCM capture threads ready");

    let session = PcmStreamSession {
        stop_tx: Some(stop_tx),
        capture_thread: Some(capture_thread),
        forward_thread: Some(forward_thread),
        #[cfg(feature = "test-support")]
        test_hold_tx: None,
    };
    if voice_processing {
        wait_for_voice_processing_ready(&stats, DUPLEX_CAPTURE_READY_TIMEOUT)?;
        tracing::info!(target: "norma_voice", "voice processing delivered first PCM chunk");
    }
    Ok((session, chunk_rx))
}

fn wait_for_voice_processing_ready(
    stats: &AudioQueueStats,
    timeout: Duration,
) -> Result<(), VoiceInputError> {
    let deadline = Instant::now() + timeout;
    while stats.sent_chunks() == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    if stats.sent_chunks() == 0 {
        tracing::warn!(target: "norma_voice", timeout_ms = timeout.as_millis(),
            "voice processing produced no PCM; refusing capture without echo cancellation");
        return Err(VoiceInputError::Audio(
            "回声消除未能启动，请重新开启语音对话".into(),
        ));
    }
    Ok(())
}

fn spawn_capture_thread(
    pcm_tx: crossbeam_channel::Sender<PcmEvent>,
    stop_rx: mpsc::Receiver<()>,
    started_tx: mpsc::Sender<Result<(), VoiceInputError>>,
    input_device_id: Option<String>,
    voice_processing: bool,
    stats: Arc<AudioQueueStats>,
) -> Result<JoinHandle<()>, VoiceInputError> {
    std::thread::Builder::new()
        .name("voice_input_pcm_capture".to_string())
        .spawn(move || {
            let run = || {
                let capture = AudioCapture::start_with_voice_processing(
                    pcm_tx,
                    stats.clone(),
                    input_device_id.as_deref(),
                    PCM_STREAM_SAMPLE_RATE_HZ,
                    voice_processing,
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
                            voice_processing,
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
            };
            // std::thread 没有 Cocoa autorelease pool；必须在会话结束前释放旧 IO 对象。
            #[cfg(target_os = "macos")]
            objc2::rc::autoreleasepool(|_| run());
            #[cfg(not(target_os = "macos"))]
            run();
        })
        .map_err(|error| {
            VoiceInputError::Audio(format!("could not spawn pcm capture thread: {error}"))
        })
}

fn spawn_forward_thread(
    pcm_rx: crossbeam_channel::Receiver<PcmEvent>,
    chunk_tx: async_channel::Sender<PcmAudioChunk>,
    stale_rx: Option<async_channel::Receiver<PcmAudioChunk>>,
) -> Result<JoinHandle<()>, VoiceInputError> {
    std::thread::Builder::new()
        .name("voice_input_pcm_forwarder".to_string())
        .spawn(move || forward_pcm_events(pcm_rx, chunk_tx, stale_rx))
        .map_err(|error| {
            VoiceInputError::Audio(format!(
                "could not spawn pcm stream forwarder thread: {error}"
            ))
        })
}

fn forward_pcm_events(
    pcm_rx: crossbeam_channel::Receiver<PcmEvent>,
    chunk_tx: async_channel::Sender<PcmAudioChunk>,
    stale_rx: Option<async_channel::Receiver<PcmAudioChunk>>,
) {
    let mut first_chunk_forwarded = false;
    while let Ok(event) = pcm_rx.recv() {
        match event {
            PcmEvent::Samples(samples) => {
                let Some(chunk) = pcm_stream_chunk_from_samples(&samples) else {
                    continue;
                };
                let sent = match chunk_tx.try_send(chunk) {
                    Err(async_channel::TrySendError::Full(chunk)) if stale_rx.is_some() => {
                        if let Some(receiver) = &stale_rx {
                            let _ = receiver.try_recv();
                        }
                        chunk_tx.try_send(chunk)
                    }
                    result => result,
                };
                match sent {
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
