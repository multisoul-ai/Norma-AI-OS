use crate::audio::AudioCapture;
use crate::permissions::ensure_microphone_authorized;
use crate::session_state::{SessionEvent, SessionState};
use crate::SpeechError;
use crossbeam_channel::bounded;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) const PCM_QUEUE_CHUNKS: usize = 30;
pub(crate) const TAP_DRAIN_WAIT_MS: u64 = 200;
pub(crate) const FINALIZE_TIMEOUT_MS: u64 = 3_000;
const RECOGNIZER_IDLE_SLEEP_MS: u64 = 10;

pub(crate) struct InnerSession {
    audio: Option<AudioCapture>,
    state: Arc<Mutex<SessionState>>,
    thread_handle: Option<std::thread::JoinHandle<String>>,
    pcm_tx: Option<crossbeam_channel::Sender<PcmEvent>>,
    abort_tx: crossbeam_channel::Sender<()>,
    stats: Arc<AudioQueueStats>,
}

#[derive(Debug)]
pub(crate) enum PcmEvent {
    Samples(Vec<f32>),
    EndOfAudio,
}

#[derive(Debug, Default)]
pub(crate) struct AudioQueueStats {
    sent_chunks: AtomicUsize,
    dropped_chunks: AtomicUsize,
}

impl AudioQueueStats {
    pub(crate) fn record_sent_chunk(&self) {
        self.sent_chunks.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_dropped_chunk(&self) {
        self.dropped_chunks.fetch_add(1, Ordering::Relaxed);
    }

    fn sent_chunks(&self) -> usize {
        self.sent_chunks.load(Ordering::Relaxed)
    }

    fn dropped_chunks(&self) -> usize {
        self.dropped_chunks.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FinalizeOutcome {
    Final(String),
    TimedOut(String),
    Aborted,
}

pub(crate) trait FinalizeRecognizer {
    fn accept_samples(&mut self, samples: &[f32]);
    fn input_finished(&mut self);
    fn is_ready(&self) -> bool;
    fn decode(&mut self);
    fn result_text(&self) -> String;
}

impl InnerSession {
    pub(crate) fn finish(mut self) -> Result<String, SpeechError> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| SpeechError::Recognizer("state lock poisoned".to_string()))?;
            state.apply(SessionEvent::Finish)?;
        }
        // Stop audio first so no more PCM arrives.
        if let Some(capture) = self.audio.take() {
            capture.stop();
        }
        std::thread::sleep(Duration::from_millis(TAP_DRAIN_WAIT_MS));
        self.finish_pcm_input();
        // Wait for recognizer thread to return the final transcript.
        match self.thread_handle.take() {
            Some(handle) => handle
                .join()
                .map_err(|_| SpeechError::Recognizer("recognizer thread panicked".to_string()))
                .and_then(|s| {
                    if s.is_empty() {
                        Err(SpeechError::EmptyTranscript)
                    } else {
                        Ok(s)
                    }
                }),
            None => Err(SpeechError::EmptyTranscript),
        }
    }

    pub(crate) fn cancel(mut self) {
        let _ = self.state.lock().map(|mut s| s.apply(SessionEvent::Cancel));
        if let Some(capture) = self.audio.take() {
            capture.stop();
        }
        let _ = self.abort_tx.try_send(());
        self.pcm_tx.take();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    fn finish_pcm_input(&mut self) {
        let Some(tx) = self.pcm_tx.take() else {
            return;
        };
        match tx.try_send(PcmEvent::EndOfAudio) {
            Ok(()) => {}
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                tracing::warn!(
                    target: "voice_sherpa::session",
                    sent_chunks = self.stats.sent_chunks(),
                    dropped_chunks = self.stats.dropped_chunks(),
                    "voice PCM queue full while sending EndOfAudio; using channel disconnect as graceful end"
                );
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {}
        }
    }
}

impl Drop for InnerSession {
    fn drop(&mut self) {
        // Drop is cleanup/abort, not graceful finalization.
        let _ = self.abort_tx.try_send(());
        self.pcm_tx.take();
        // Join the thread to prevent it from spinning after the session is dropped.
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

pub(crate) fn start(
    model_dir: PathBuf,
) -> Result<(crate::SpeechSession, async_channel::Receiver<String>), SpeechError> {
    ensure_microphone_authorized()?;

    let mut state = SessionState::default();
    state.apply(SessionEvent::Start)?;
    let state = Arc::new(Mutex::new(state));

    // PCM channel: audio tap → recognizer thread.
    let (pcm_tx, pcm_rx) = bounded::<PcmEvent>(PCM_QUEUE_CHUNKS);
    // Partials channel: recognizer thread → caller.
    // bounded(64): sherpa emits ~10 partials/s; 64 slots is more than enough.
    // When the channel is closed, recv().await returns Err — safe for GPUI tasks.
    let (partial_tx, partial_rx) = async_channel::bounded::<String>(64);
    // Abort signal: cancel/drop → recognizer thread. finish() uses EndOfAudio instead.
    let (abort_tx, abort_rx) = bounded::<()>(1);
    let stats = Arc::new(AudioQueueStats::default());

    let config = build_recognizer_config(&model_dir)?;
    let recognizer_stats = Arc::clone(&stats);

    let thread_handle = std::thread::Builder::new()
        .name("voice_sherpa_recognizer".to_string())
        .spawn(move || recognizer_loop(config, pcm_rx, abort_rx, partial_tx, recognizer_stats))
        .map_err(|e| SpeechError::Recognizer(format!("could not spawn recognizer thread: {e}")))?;

    let audio = AudioCapture::start(pcm_tx.clone(), Arc::clone(&stats))?;

    Ok((
        crate::SpeechSession {
            inner: InnerSession {
                audio: Some(audio),
                state,
                thread_handle: Some(thread_handle),
                pcm_tx: Some(pcm_tx),
                abort_tx,
                stats,
            },
        },
        partial_rx,
    ))
}

fn build_recognizer_config(
    model_dir: &Path,
) -> Result<sherpa_onnx::OnlineRecognizerConfig, SpeechError> {
    use sherpa_onnx::{OnlineModelConfig, OnlineParaformerModelConfig, OnlineRecognizerConfig};

    // Try int8 first, then full precision.
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

fn find_model_file(dir: &Path, candidates: &[&str]) -> Result<PathBuf, SpeechError> {
    for name in candidates {
        let path = dir.join(name);
        if path.exists() {
            return Ok(path);
        }
    }
    Err(SpeechError::ModelNotFound(format!(
        "none of {:?} found in {}",
        candidates,
        dir.display()
    )))
}

fn recognizer_loop(
    config: sherpa_onnx::OnlineRecognizerConfig,
    pcm_rx: crossbeam_channel::Receiver<PcmEvent>,
    abort_rx: crossbeam_channel::Receiver<()>,
    partial_tx: async_channel::Sender<String>,
    stats: Arc<AudioQueueStats>,
) -> String {
    let mut recognizer = match SherpaFinalizeRecognizer::create(&config) {
        Some(recognizer) => recognizer,
        None => {
            tracing::error!(target: "voice_sherpa::session", "failed to create OnlineRecognizer");
            return String::new();
        }
    };

    let started = Instant::now();
    let outcome = run_finalize(
        &mut recognizer,
        &pcm_rx,
        &abort_rx,
        Some(&partial_tx),
        Duration::from_millis(FINALIZE_TIMEOUT_MS),
    );
    let elapsed_ms = started.elapsed().as_millis();
    match outcome {
        Ok(FinalizeOutcome::Final(text)) => {
            tracing::info!(
                target: "voice_sherpa::session",
                elapsed_ms,
                sent_chunks = stats.sent_chunks(),
                dropped_chunks = stats.dropped_chunks(),
                "voice finalization completed"
            );
            send_final_partial(&partial_tx, &text);
            text
        }
        Ok(FinalizeOutcome::TimedOut(text)) => {
            tracing::warn!(
                target: "voice_sherpa::session",
                elapsed_ms,
                sent_chunks = stats.sent_chunks(),
                dropped_chunks = stats.dropped_chunks(),
                transcript_chars = text.chars().count(),
                "voice finalization timed out; returning best transcript"
            );
            send_final_partial(&partial_tx, &text);
            text
        }
        Ok(FinalizeOutcome::Aborted) => {
            tracing::info!(
                target: "voice_sherpa::session",
                elapsed_ms,
                sent_chunks = stats.sent_chunks(),
                dropped_chunks = stats.dropped_chunks(),
                "voice finalization aborted"
            );
            String::new()
        }
        Err(error) => {
            tracing::warn!(
                target: "voice_sherpa::session",
                elapsed_ms,
                sent_chunks = stats.sent_chunks(),
                dropped_chunks = stats.dropped_chunks(),
                %error,
                "voice finalization failed"
            );
            String::new()
        }
    }
}

struct SherpaFinalizeRecognizer {
    recognizer: sherpa_onnx::OnlineRecognizer,
    stream: sherpa_onnx::OnlineStream,
}

impl SherpaFinalizeRecognizer {
    fn create(config: &sherpa_onnx::OnlineRecognizerConfig) -> Option<Self> {
        let recognizer = sherpa_onnx::OnlineRecognizer::create(config)?;
        let stream = recognizer.create_stream();
        Some(Self { recognizer, stream })
    }
}

impl FinalizeRecognizer for SherpaFinalizeRecognizer {
    fn accept_samples(&mut self, samples: &[f32]) {
        self.stream.accept_waveform(16_000, samples);
    }

    fn input_finished(&mut self) {
        self.stream.input_finished();
    }

    fn is_ready(&self) -> bool {
        self.recognizer.is_ready(&self.stream)
    }

    fn decode(&mut self) {
        self.recognizer.decode(&self.stream);
    }

    fn result_text(&self) -> String {
        self.recognizer
            .get_result(&self.stream)
            .map(|result| result.text.trim().to_string())
            .unwrap_or_default()
    }
}

fn run_finalize<R: FinalizeRecognizer>(
    recognizer: &mut R,
    pcm_rx: &crossbeam_channel::Receiver<PcmEvent>,
    abort_rx: &crossbeam_channel::Receiver<()>,
    partial_tx: Option<&async_channel::Sender<String>>,
    finalize_timeout: Duration,
) -> Result<FinalizeOutcome, SpeechError> {
    let mut last_text = String::new();

    loop {
        if abort_rx.try_recv().is_ok() {
            return Ok(FinalizeOutcome::Aborted);
        }
        match pcm_rx.recv_timeout(Duration::from_millis(RECOGNIZER_IDLE_SLEEP_MS)) {
            Ok(PcmEvent::Samples(samples)) => {
                if let Some(outcome) =
                    accept_and_decode(recognizer, &samples, partial_tx, &mut last_text, None)?
                {
                    return Ok(outcome);
                }
            }
            Ok(PcmEvent::EndOfAudio) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }

    let deadline = Instant::now() + finalize_timeout;
    loop {
        if abort_rx.try_recv().is_ok() {
            return Ok(FinalizeOutcome::Aborted);
        }
        match pcm_rx.try_recv() {
            Ok(PcmEvent::Samples(samples)) => {
                if let Some(outcome) = accept_and_decode(
                    recognizer,
                    &samples,
                    partial_tx,
                    &mut last_text,
                    Some(deadline),
                )? {
                    return Ok(outcome);
                }
            }
            Ok(PcmEvent::EndOfAudio) => {}
            Err(crossbeam_channel::TryRecvError::Empty)
            | Err(crossbeam_channel::TryRecvError::Disconnected) => break,
        }
    }

    if let Some(outcome) = decode_ready(recognizer, partial_tx, &mut last_text, Some(deadline))? {
        return Ok(outcome);
    }
    recognizer.input_finished();
    if let Some(outcome) = decode_ready(recognizer, partial_tx, &mut last_text, Some(deadline))? {
        return Ok(outcome);
    }

    final_outcome(recognizer, false)
}

fn accept_and_decode<R: FinalizeRecognizer>(
    recognizer: &mut R,
    samples: &[f32],
    partial_tx: Option<&async_channel::Sender<String>>,
    last_text: &mut String,
    deadline: Option<Instant>,
) -> Result<Option<FinalizeOutcome>, SpeechError> {
    recognizer.accept_samples(samples);
    decode_ready(recognizer, partial_tx, last_text, deadline)
}

fn decode_ready<R: FinalizeRecognizer>(
    recognizer: &mut R,
    partial_tx: Option<&async_channel::Sender<String>>,
    last_text: &mut String,
    deadline: Option<Instant>,
) -> Result<Option<FinalizeOutcome>, SpeechError> {
    while recognizer.is_ready() {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return final_outcome(recognizer, true).map(Some);
        }
        recognizer.decode();
        emit_partial_if_changed(recognizer, partial_tx, last_text);
    }
    emit_partial_if_changed(recognizer, partial_tx, last_text);
    Ok(None)
}

fn final_outcome<R: FinalizeRecognizer>(
    recognizer: &R,
    timed_out: bool,
) -> Result<FinalizeOutcome, SpeechError> {
    let text = recognizer.result_text().trim().to_string();
    if text.is_empty() {
        return Err(SpeechError::EmptyTranscript);
    }
    if timed_out {
        Ok(FinalizeOutcome::TimedOut(text))
    } else {
        Ok(FinalizeOutcome::Final(text))
    }
}

fn emit_partial_if_changed<R: FinalizeRecognizer>(
    recognizer: &R,
    partial_tx: Option<&async_channel::Sender<String>>,
    last_text: &mut String,
) {
    let text = recognizer.result_text().trim().to_string();
    if text.is_empty() || text == *last_text {
        return;
    }
    *last_text = text.clone();
    if let Some(partial_tx) = partial_tx {
        let _ = partial_tx.try_send(text);
    }
}

fn send_final_partial(partial_tx: &async_channel::Sender<String>, text: &str) {
    if !text.trim().is_empty() {
        let _ = partial_tx.try_send(text.trim().to_string());
    }
}

#[cfg(test)]
fn run_finalize_for_test<R: FinalizeRecognizer>(
    recognizer: &mut R,
    pcm_rx: &crossbeam_channel::Receiver<PcmEvent>,
    timeout: Duration,
) -> Result<FinalizeOutcome, SpeechError> {
    let (_abort_tx, abort_rx) = bounded::<()>(1);
    run_finalize(recognizer, pcm_rx, &abort_rx, None, timeout)
}

#[cfg(test)]
fn run_finalize_with_abort_for_test<R: FinalizeRecognizer>(
    recognizer: &mut R,
    pcm_rx: &crossbeam_channel::Receiver<PcmEvent>,
    abort_rx: &crossbeam_channel::Receiver<()>,
    timeout: Duration,
) -> Result<FinalizeOutcome, SpeechError> {
    run_finalize(recognizer, pcm_rx, abort_rx, None, timeout)
}

#[cfg(test)]
#[path = "../tests/unit/finalize_tests.rs"]
mod finalize_tests;
