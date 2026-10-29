use super::{
    config::error,
    transport::{Guard, ProcessControl, Transport},
    A1Config,
};
use crate::{
    AsrError, AsrProviderCapabilities, RecognitionCancellation, Recognizer, RecognizerFactory,
};
use audio::PcmAudioChunk;
use std::{
    io::Write,
    process::Command,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct State {
    idle: Option<Engine>,
    active: Option<RecognitionCancellation>,
    children: Vec<ProcessControl>,
    next_session: u64,
    releases: usize,
    ready: bool,
}

/// One process pair and one active utterance across all windows; no waiting queue.
#[derive(Default)]
pub struct A1EngineHost {
    state: Arc<Mutex<State>>,
}
static HOST: OnceLock<Arc<A1EngineHost>> = OnceLock::new();

#[must_use = "move A1Release to a background executor and call wait"]
pub struct A1Release {
    idle: Option<Engine>,
    children: Vec<ProcessControl>,
    state: Option<Arc<Mutex<State>>>,
}
impl A1Release {
    pub fn wait(self) {
        drop(self);
    }
}
impl Drop for A1Release {
    fn drop(&mut self) {
        drop(self.idle.take());
        for child in self.children.drain(..) {
            child.terminate();
        }
        if let Some(state) = self.state.take() {
            state.lock().unwrap_or_else(|e| e.into_inner()).releases -= 1;
        }
    }
}
/// Invalidate synchronously; move the returned cleanup ticket to a background executor.
pub fn begin_shared_a1_shutdown() -> A1Release {
    HOST.get().map_or(
        A1Release {
            idle: None,
            children: Vec::new(),
            state: None,
        },
        |host| host.begin_shutdown(),
    )
}
pub fn shared_a1_ready() -> bool {
    HOST.get().is_some_and(|host| {
        host.state.try_lock().is_ok_and(|state| {
            state.ready
                && state.releases == 0
                && state
                    .active
                    .as_ref()
                    .is_none_or(|token| !token.is_cancelled())
        })
    })
}
pub fn shutdown_shared_a1() {
    begin_shared_a1_shutdown().wait();
}

impl A1EngineHost {
    fn register(&self, child: ProcessControl) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state
            .active
            .as_ref()
            .is_none_or(|token| token.is_cancelled())
        {
            drop(state);
            child.terminate();
        } else {
            state.children.push(child);
        }
    }
    /// Blocking process cleanup. Always call from a background executor.
    pub fn shutdown(&self) {
        self.begin_shutdown().wait();
    }
    fn begin_shutdown(&self) -> A1Release {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(active) = &state.active {
            active.cancel();
        }
        state.ready = false;
        state.releases += 1;
        A1Release {
            idle: state.idle.take(),
            children: state.children.clone(),
            state: Some(self.state.clone()),
        }
    }
}

#[derive(Clone)]
pub struct A1RecognizerFactory {
    config: A1Config,
    host: Arc<A1EngineHost>,
}
impl A1RecognizerFactory {
    /// Loads both models without running transcription; keep the successful empty session warm.
    pub fn prepare(&self) -> Result<(), AsrError> {
        self.prepare_with_cancellation(RecognitionCancellation::new())
    }
    pub fn prepare_with_cancellation(
        &self,
        cancellation: RecognitionCancellation,
    ) -> Result<(), AsrError> {
        if !super::resources::normal_pressure() {
            return Err(error("system memory pressure is elevated"));
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if cancellation.is_cancelled() {
                return Err(AsrError::SessionClosed);
            }
            let state = self.host.state.lock().map_err(error)?;
            let cancelled_active = state
                .active
                .as_ref()
                .is_some_and(|token| token.is_cancelled());
            if state.releases == 0 && !cancelled_active {
                if state
                    .idle
                    .as_ref()
                    .is_some_and(|engine| engine.config == self.config)
                {
                    return Ok(());
                }
                break;
            }
            drop(state);
            if Instant::now() >= deadline {
                return Err(error("cleanup timeout"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut recognizer = self.build_recognizer(cancellation)?;
        recognizer.finish()?;
        Ok(())
    }
    pub fn new(config: A1Config) -> Self {
        Self {
            config,
            host: HOST
                .get_or_init(|| {
                    let host = Arc::new(A1EngineHost::default());
                    super::resources::watch(Arc::downgrade(&host));
                    host
                })
                .clone(),
        }
    }
}
impl RecognizerFactory for A1RecognizerFactory {
    fn capabilities(&self) -> AsrProviderCapabilities {
        AsrProviderCapabilities::new(true, Some(60))
    }
    fn create(&self) -> Result<Box<dyn Recognizer>, AsrError> {
        self.build_recognizer(RecognitionCancellation::new())
    }
    fn create_with_cancellation(
        &self,
        cancellation: RecognitionCancellation,
    ) -> Result<Box<dyn Recognizer>, AsrError> {
        self.build_recognizer(cancellation)
    }
}
impl A1RecognizerFactory {
    fn build_recognizer(
        &self,
        cancellation: RecognitionCancellation,
    ) -> Result<Box<dyn Recognizer>, AsrError> {
        if cancellation.is_cancelled() {
            return Err(AsrError::SessionClosed);
        }
        if !super::resources::normal_pressure() {
            return Err(error("system memory pressure is elevated"));
        }
        let host_cancel = RecognitionCancellation::new();
        let (cached, session) = {
            let mut state = self.host.state.lock().map_err(error)?;
            if state.active.is_some() || state.releases != 0 {
                return Err(error("another A1 recording is active"));
            }
            state.active = Some(host_cancel.clone());
            state.next_session = state
                .next_session
                .checked_add(1)
                .ok_or_else(|| error("session counter exhausted"))?;
            (state.idle.take(), state.next_session)
        };
        // From here RAII owns the reservation, including startup/handshake failure.
        let mut recognizer = A1Recognizer {
            host: self.host.clone(),
            guard: Guard {
                host: host_cancel,
                session: cancellation,
            },
            engine: None,
            session,
            sequence: 0,
            samples: 0,
            terminal: false,
            reusable: false,
            last_partial: String::new(),
        };
        recognizer.engine = Some(match cached {
            Some(engine) if engine.config == self.config => engine,
            other => {
                drop(other);
                self.host.state.lock().map_err(error)?.children.clear();
                Engine::start(&self.config, &recognizer.guard, &self.host)?
            }
        });
        recognizer.exchange("begin", &[], REQUEST_TIMEOUT)?;
        self.host.state.lock().map_err(error)?.ready = true;
        Ok(Box::new(recognizer))
    }
}

struct Engine {
    config: A1Config,
    preview: Transport,
    finalizer: Transport,
    _script: Option<tempfile::NamedTempFile>,
}
impl Engine {
    fn start(config: &A1Config, guard: &Guard, host: &A1EngineHost) -> Result<Self, AsrError> {
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err(error("A1 requires Apple Silicon macOS"));
        }
        guard.check()?;
        config.validate()?;
        guard.check()?;
        let mut script = tempfile::Builder::new()
            .prefix("norma-a1-")
            .suffix(".py")
            .tempfile()
            .map_err(error)?;
        script
            .write_all(include_bytes!("../../../../runtime/a1/worker.py"))
            .map_err(error)?;
        script.flush().map_err(error)?;
        let launch = |role, model: &std::path::Path| {
            let mut command = Command::new("/usr/bin/sandbox-exec");
            command
                .args(["-p", "(version 1)(allow default)(deny network*)"])
                .arg(&config.python)
                .args(["-I", "-B", "-u"])
                .arg(script.path())
                .arg("--role")
                .arg(role)
                .arg("--model")
                .arg(model)
                .arg("--parent")
                .arg(std::process::id().to_string())
                .env("HF_HUB_OFFLINE", "1")
                .env("TRANSFORMERS_OFFLINE", "1")
                .env("HF_HUB_DISABLE_TELEMETRY", "1")
                .env("TOKENIZERS_PARALLELISM", "false")
                .env("OMP_NUM_THREADS", "2")
                .env("OPENBLAS_NUM_THREADS", "2")
                .env("VECLIB_MAXIMUM_THREADS", "2")
                .env_remove("PYTHONPATH");
            Transport::spawn_registered(command, role, guard, Duration::from_secs(60), |child| {
                host.register(child)
            })
        };
        let preview = launch("zipformer", &config.zipformer)?;
        let finalizer = launch("qwen", &config.qwen)?;
        Ok(Self {
            config: config.clone(),
            preview,
            finalizer,
            _script: Some(script),
        })
    }
}

struct A1Recognizer {
    host: Arc<A1EngineHost>,
    guard: Guard,
    engine: Option<Engine>,
    session: u64,
    sequence: u64,
    samples: usize,
    terminal: bool,
    reusable: bool,
    last_partial: String,
}
impl A1Recognizer {
    fn exchange(
        &mut self,
        op: &str,
        pcm: &[u8],
        timeout: Duration,
    ) -> Result<(String, String), AsrError> {
        self.guard.check()?;
        self.sequence += 1;
        let engine = self.engine.as_mut().ok_or(AsrError::SessionClosed)?;
        let deadline = Instant::now() + timeout;
        let result = (|| {
            let preview = engine.preview.request(
                self.session,
                self.sequence,
                op,
                pcm,
                &self.guard,
                timeout,
            )?;
            let final_text = engine.finalizer.request(
                self.session,
                self.sequence,
                op,
                pcm,
                &self.guard,
                deadline.saturating_duration_since(Instant::now()),
            )?;
            Ok((preview, final_text))
        })();
        if result.is_err() {
            self.terminal = true;
            self.reusable = false;
            self.engine.take();
        }
        result
    }
}
impl Recognizer for A1Recognizer {
    fn set_cancellation_token(&mut self, cancellation: RecognitionCancellation) {
        self.guard.session = cancellation;
    }
    fn accept_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<String>, AsrError> {
        if self.terminal {
            return Err(AsrError::SessionClosed);
        }
        if chunk.sample_rate_hz != 16000
            || chunk.channels != 1
            || !chunk.bytes.len().is_multiple_of(2)
            || self.samples + chunk.bytes.len() / 2 > 960_000
        {
            self.terminal = true;
            return Err(AsrError::InvalidPcm);
        }
        let mut partials = Vec::new();
        for bytes in chunk.bytes.chunks(32_000) {
            let (preview, _) = self.exchange("audio", bytes, REQUEST_TIMEOUT)?;
            self.samples += bytes.len() / 2;
            if !preview.trim().is_empty() && preview != self.last_partial {
                self.last_partial = preview.clone();
                partials.push(preview);
            }
        }
        Ok(partials)
    }
    fn finish(&mut self) -> Result<Option<String>, AsrError> {
        if self.terminal {
            return Err(AsrError::SessionClosed);
        }
        self.terminal = true;
        let (_, text) = self.exchange("finish", &[], REQUEST_TIMEOUT)?;
        self.guard.check()?;
        self.reusable = true;
        Ok((!text.trim().is_empty()).then(|| text.trim().to_owned()))
    }
}
impl Drop for A1Recognizer {
    fn drop(&mut self) {
        let engine = self.engine.take();
        if self.reusable && self.guard.check().is_ok() {
            let mut state = self.host.state.lock().unwrap_or_else(|e| e.into_inner());
            // Check again inside the lock so a concurrent shutdown cannot re-cache a model.
            if self.guard.check().is_ok() {
                state.idle = engine;
                state.active = None;
                return;
            }
            drop(state);
        }
        drop(engine); // kill/wait before making the slot available
        let mut state = self.host.state.lock().unwrap_or_else(|e| e.into_inner());
        state.children.clear();
        state.ready = false;
        state.active = None;
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/a1_host_tests.rs"]
mod tests;
