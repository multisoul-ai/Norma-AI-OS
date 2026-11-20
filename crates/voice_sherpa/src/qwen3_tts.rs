//! Qwen3-TTS on the managed A1 Python engine; one shared worker keeps the model loaded.
use audio::{PcmAudioChunk, PcmFormat};
use std::{
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::Mutex,
};

const WORKER_SOURCE: &str = include_str!("../runtime/qwen3_tts_worker.py");
const SAMPLE_RATE_HZ: u32 = 24_000;
const END: u32 = 0;
const ERROR: u32 = u32::MAX;
// 0.32 s streaming interval × 24 kHz × 2 B is ~15 KB; anything near 4 MB is a broken stream.
const MAX_FRAME: u32 = 4 * 1024 * 1024;

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

/// Kills the shared worker. Blocks while a sentence is synthesizing: call off the foreground.
pub fn shutdown_qwen3_tts() {
    drop(lock().take());
}

fn lock() -> std::sync::MutexGuard<'static, Option<Worker>> {
    WORKER.lock().unwrap_or_else(|error| error.into_inner())
}

fn error(kind: tts::TtsErrorKind, message: impl Into<String>) -> tts::TtsError {
    tts::TtsError::new(kind, message)
}

pub struct Qwen3TtsClient {
    python: PathBuf,
    model: PathBuf,
    voice: String,
    pub(crate) stream: Option<crate::tts::LocalStreamState>,
}

impl Qwen3TtsClient {
    pub fn new(python: PathBuf, model: &tts::AvailableTtsModel, voice: &tts::TtsVoiceId) -> Self {
        Self {
            python,
            model: model.root().to_path_buf(),
            voice: voice.as_str().to_string(),
            stream: None,
        }
    }

    /// Blocking: runs on the reply thread like the Sherpa clients. `deliver` false cancels.
    pub(crate) fn synthesize(
        &self,
        text: &str,
        deliver: impl FnMut(PcmAudioChunk) -> bool,
    ) -> Result<(), tts::TtsError> {
        synthesize_with(
            WORKER_SOURCE,
            &self.python,
            &self.model,
            &self.voice,
            text,
            deliver,
        )
    }
}

pub(crate) fn synthesize_with(
    source: &'static str,
    python: &Path,
    model: &Path,
    voice: &str,
    text: &str,
    deliver: impl FnMut(PcmAudioChunk) -> bool,
) -> Result<(), tts::TtsError> {
    let mut slot = lock();
    let key = (source, python.to_path_buf(), model.to_path_buf());
    if slot.as_ref().is_none_or(|worker| worker.key != key) {
        *slot = None;
        *slot = Some(Worker::start(key)?);
    }
    let Some(worker) = slot.as_mut() else {
        return Err(error(
            tts::TtsErrorKind::Configuration,
            "Qwen3-TTS worker unavailable",
        ));
    };
    let result = worker.speak(text, voice, deliver);
    if result.is_err() {
        // A broken or failed stream is not trusted for the next reply; restart lazily.
        *slot = None;
    }
    result
}

struct Worker {
    key: (&'static str, PathBuf, PathBuf),
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Worker {
    fn start(key: (&'static str, PathBuf, PathBuf)) -> Result<Self, tts::TtsError> {
        let transport = |message: &str| error(tts::TtsErrorKind::Transport, message);
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command
            .args(["-p", "(version 1)(allow default)(deny network*)"])
            .arg(&key.1)
            .args(["-I", "-B", "-u", "-c", key.0, "--model"])
            .arg(&key.2)
            .arg("--parent")
            .arg(std::process::id().to_string())
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("HF_HUB_DISABLE_TELEMETRY", "1")
            .env("TOKENIZERS_PARALLELISM", "false")
            .env_remove("PYTHONPATH")
            .env_remove("PYTHONHOME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .map_err(|_| transport("could not start the Qwen3-TTS worker"))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(transport("Qwen3-TTS worker pipes unavailable"));
        };
        let mut worker = Self {
            key,
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
        };
        match worker.read_frame_length() {
            Ok(END) => Ok(worker),
            _ => Err(transport("Qwen3-TTS model failed to load")),
        }
    }

    fn read_frame_length(&mut self) -> Result<u32, tts::TtsError> {
        let mut length = [0u8; 4];
        self.stdout
            .read_exact(&mut length)
            .map_err(|_| error(tts::TtsErrorKind::Transport, "Qwen3-TTS worker exited"))?;
        Ok(u32::from_le_bytes(length))
    }

    fn send(&mut self, message: serde_json::Value) -> Result<(), tts::TtsError> {
        writeln!(self.stdin, "{message}")
            .and_then(|_| self.stdin.flush())
            .map_err(|_| error(tts::TtsErrorKind::Transport, "Qwen3-TTS worker exited"))
    }

    fn speak(
        &mut self,
        text: &str,
        voice: &str,
        mut deliver: impl FnMut(PcmAudioChunk) -> bool,
    ) -> Result<(), tts::TtsError> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(serde_json::json!({ "id": id, "text": text, "voice": voice }))?;
        let mut wanted = true;
        loop {
            let length = match self.read_frame_length()? {
                END => return Ok(()),
                ERROR => {
                    return Err(error(
                        tts::TtsErrorKind::Protocol,
                        "Qwen3-TTS synthesis failed",
                    ))
                }
                length if length > MAX_FRAME || length % 2 == 1 => {
                    return Err(error(
                        tts::TtsErrorKind::Protocol,
                        "invalid Qwen3-TTS frame",
                    ))
                }
                length => length,
            };
            let mut bytes = vec![0u8; length as usize];
            self.stdout
                .read_exact(&mut bytes)
                .map_err(|_| error(tts::TtsErrorKind::Transport, "Qwen3-TTS worker exited"))?;
            // After a cancel, keep draining so the next request starts on a frame boundary.
            if wanted {
                let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, SAMPLE_RATE_HZ, 1, bytes)
                    .map_err(|_| error(tts::TtsErrorKind::AudioDecode, "invalid Qwen3-TTS PCM"))?;
                if !deliver(chunk) {
                    wanted = false;
                    self.send(serde_json::json!({ "cancel": id }))?;
                }
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
#[path = "../tests/unit/qwen3_tts_tests.rs"]
mod tests;
