use crate::audio::AudioCapture;
use crate::permissions::ensure_microphone_authorized;
use crate::session_state::{SessionEvent, SessionState};
use crate::SpeechError;
use crossbeam_channel::unbounded;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub(crate) struct InnerSession {
    audio: Option<AudioCapture>,
    state: Arc<Mutex<SessionState>>,
    thread_handle: Option<std::thread::JoinHandle<String>>,
    stop_tx: crossbeam_channel::Sender<()>,
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
        // Signal recognizer thread to flush and exit.
        let _ = self.stop_tx.send(());
        // Wait for recognizer thread to return the final transcript.
        match self.thread_handle.take() {
            Some(handle) => handle
                .join()
                .map_err(|_| SpeechError::Recognizer("recognizer thread panicked".to_string())),
            None => Err(SpeechError::EmptyTranscript),
        }
    }

    pub(crate) fn cancel(mut self) {
        let _ = self.state.lock().map(|mut s| s.apply(SessionEvent::Cancel));
        if let Some(capture) = self.audio.take() {
            capture.stop();
        }
        let _ = self.stop_tx.send(());
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

pub(crate) fn start(model_dir: PathBuf) -> Result<crate::SpeechSession, SpeechError> {
    ensure_microphone_authorized()?;

    let mut state = SessionState::default();
    state.apply(SessionEvent::Start)?;
    let state = Arc::new(Mutex::new(state));

    // PCM channel: audio tap → recognizer thread.
    let (pcm_tx, pcm_rx) = unbounded::<Vec<f32>>();
    // Partials channel: recognizer thread → caller.
    let (partial_tx, partial_rx) = std::sync::mpsc::channel::<String>();
    // Stop signal: session::finish/cancel → recognizer thread.
    let (stop_tx, stop_rx) = unbounded::<()>();

    let config = build_recognizer_config(&model_dir)?;

    let thread_handle = std::thread::Builder::new()
        .name("voice_sherpa_recognizer".to_string())
        .spawn(move || recognizer_loop(config, pcm_rx, stop_rx, partial_tx))
        .map_err(|e| SpeechError::Recognizer(format!("could not spawn recognizer thread: {e}")))?;

    let audio = AudioCapture::start(pcm_tx)?;

    Ok(crate::SpeechSession {
        partials: partial_rx,
        inner: InnerSession {
            audio: Some(audio),
            state,
            thread_handle: Some(thread_handle),
            stop_tx,
        },
    })
}

fn build_recognizer_config(
    model_dir: &PathBuf,
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

fn find_model_file(dir: &PathBuf, candidates: &[&str]) -> Result<PathBuf, SpeechError> {
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
    pcm_rx: crossbeam_channel::Receiver<Vec<f32>>,
    stop_rx: crossbeam_channel::Receiver<()>,
    partial_tx: std::sync::mpsc::Sender<String>,
) -> String {
    let recognizer = match sherpa_onnx::OnlineRecognizer::create(&config) {
        Some(r) => r,
        None => {
            eprintln!("voice_sherpa: failed to create OnlineRecognizer");
            return String::new();
        }
    };
    let stream = recognizer.create_stream();
    let mut last_text = String::new();

    loop {
        // Check for stop signal (non-blocking).
        if stop_rx.try_recv().is_ok() {
            break;
        }

        // Drain all available PCM chunks, feed to stream.
        let mut got_audio = false;
        while let Ok(samples) = pcm_rx.try_recv() {
            stream.accept_waveform(16_000, &samples);
            got_audio = true;
        }

        if got_audio {
            while recognizer.is_ready(&stream) {
                recognizer.decode(&stream);
            }
            if let Some(result) = recognizer.get_result(&stream) {
                let text = result.text.trim().to_string();
                if !text.is_empty() && text != last_text {
                    last_text = text.clone();
                    let _ = partial_tx.send(text);
                }
            }
        } else {
            // Nothing to do; yield briefly to avoid spinning.
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    // Flush remaining audio after stop signal.
    while let Ok(samples) = pcm_rx.try_recv() {
        stream.accept_waveform(16_000, &samples);
    }
    stream.input_finished();
    while recognizer.is_ready(&stream) {
        recognizer.decode(&stream);
    }
    let final_text = recognizer
        .get_result(&stream)
        .map(|r| r.text.trim().to_string())
        .unwrap_or_default();
    if !final_text.is_empty() {
        let _ = partial_tx.send(final_text.clone());
    }

    final_text
}
