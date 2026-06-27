# Voice Sherpa Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `voice_macos` (Apple SFSpeechRecognizer, cloud-based) with `voice_sherpa` (sherpa-onnx + Paraformer, fully local streaming) to eliminate rate limits, latency variance, and poor Chinese accuracy.

**Architecture:** A new `voice_sherpa` crate wraps `sherpa-onnx` with an `AVAudioEngine` tap that feeds 16kHz mono PCM into a background recognizer thread via crossbeam channel. The recognizer thread sends partial text results back via `std::sync::mpsc`, exposed as `SpeechSession::partials`. The app layer polls `partials` each frame to render live subtitles.

**Tech Stack:** `sherpa-onnx 1.13.3`, `objc2-avf-audio 0.3.2` (AVAudioEngine), `objc2-av-foundation 0.3.2` (mic permission), `crossbeam-channel 0.5`, `thiserror 2`.

## Global Constraints

- No `gpui` imports in `voice_sherpa` — domain crate, must stay UI-free.
- Rust source files ≤ 500 lines; split if approaching limit.
- No `#[allow(...)]` attributes — fix root causes.
- Model is **not** committed to git (too large); a download script fetches it to `assets/models/paraformer/`.
- Target Rust edition 2021, all workspace fields inherited.
- Paraformer model expects **16kHz mono f32** audio.

---

## File Map

### Create

| Path | Responsibility |
|---|---|
| `crates/voice_sherpa/Cargo.toml` | Crate manifest with sherpa-onnx + objc2 deps |
| `crates/voice_sherpa/src/lib.rs` | Public API: `SherpaProvider`, `SpeechSession`, `SpeechError` |
| `crates/voice_sherpa/src/permissions.rs` | Microphone-only permission check/request (no SFSpeechRecognizer auth) |
| `crates/voice_sherpa/src/session_state.rs` | `SessionState` state machine: Idle→Recording→Finished/Cancelled |
| `crates/voice_sherpa/src/audio.rs` | `AudioCapture`: start/stop AVAudioEngine tap, output 16kHz f32 PCM via channel |
| `crates/voice_sherpa/src/session.rs` | `SpeechSession`: owns audio capture + recognizer thread + channels |
| `crates/voice_sherpa/src/model.rs` | `ModelPaths`: resolve model directory from env / default paths |
| `crates/voice_sherpa/tests/unit/` | Unit tests per module |
| `scripts/download-paraformer-model.sh` | Fetch model files into `assets/models/paraformer/` |

### Modify

| Path | Change |
|---|---|
| `Cargo.toml` | Swap `voice_macos` → `voice_sherpa` in workspace `members` and `[workspace.dependencies]` |
| `crates/app/Cargo.toml` | `voice_macos` → `voice_sherpa` dependency |
| `crates/app/src/app/voice/control.rs` | Swap `voice_macos::SpeechSession` → `voice_sherpa::SpeechSession`; add `partial_text: String` field |
| `crates/app/src/app/voice/mod.rs` | Replace `AppleSpeechProvider` with `SherpaProvider`; drain `partials` channel during recording |

### Delete

| Path | Why |
|---|---|
| `crates/voice_macos/` | Entire crate; replaced by `voice_sherpa` |

---

## Task 1: Download script and model files

**Files:**
- Create: `scripts/download-paraformer-model.sh`
- Create: `assets/models/paraformer/.gitkeep`
- Modify: `.gitignore` (add `assets/models/paraformer/*.onnx`, `assets/models/paraformer/*.bin`, `assets/models/paraformer/tokens.txt`)

**Interfaces:**
- Produces: model files at `assets/models/paraformer/{encoder.onnx, decoder.onnx, tokens.txt}` used by Task 6

- [ ] **Step 1: Create the download script**

```bash
#!/usr/bin/env bash
# scripts/download-paraformer-model.sh
# Downloads sherpa-onnx-streaming-paraformer-bilingual-zh-en (int8) to assets/models/paraformer/
set -euo pipefail

DEST="$(cd "$(dirname "$0")/.." && pwd)/assets/models/paraformer"
MODEL_NAME="sherpa-onnx-streaming-paraformer-bilingual-zh-en"
BASE_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models"

mkdir -p "$DEST"
cd "$DEST"

echo "Downloading $MODEL_NAME..."
curl -LO "$BASE_URL/${MODEL_NAME}.tar.bz2"
tar -xjf "${MODEL_NAME}.tar.bz2" --strip-components=1
rm "${MODEL_NAME}.tar.bz2"

echo "Model files:"
ls -lh "$DEST"
```

Save as `scripts/download-paraformer-model.sh`, then `chmod +x scripts/download-paraformer-model.sh`.

- [ ] **Step 2: Create .gitkeep and update .gitignore**

```
# add to .gitignore
assets/models/paraformer/*.onnx
assets/models/paraformer/*.bin
assets/models/paraformer/tokens.txt
```

Create `assets/models/paraformer/.gitkeep` (empty file so directory is tracked).

- [ ] **Step 3: Run the download script**

```bash
bash scripts/download-paraformer-model.sh
```

Expected output: model files in `assets/models/paraformer/`:
- `encoder.onnx` or `encoder.int8.onnx`
- `decoder.onnx` or `decoder.int8.onnx`
- `tokens.txt`

Verify with `ls -lh assets/models/paraformer/`.

- [ ] **Step 4: Commit scaffolding**

```bash
git add scripts/download-paraformer-model.sh assets/models/paraformer/.gitkeep .gitignore
git commit -m "chore: add paraformer model download script and gitignore"
```

---

## Task 2: Bootstrap `voice_sherpa` crate

**Files:**
- Create: `crates/voice_sherpa/Cargo.toml`
- Create: `crates/voice_sherpa/src/lib.rs`
- Modify: `Cargo.toml` (workspace)

**Interfaces:**
- Produces:
  - `voice_sherpa::SpeechError` (thiserror enum used by all tasks)
  - `voice_sherpa::SherpaProvider` (stub, real impl in Task 6)
  - `voice_sherpa::SpeechSession` (stub, real impl in Task 5)

- [ ] **Step 1: Write the failing crate compile test**

Create `crates/voice_sherpa/src/lib.rs`:

```rust
pub mod permissions;
pub mod session_state;

mod audio;
mod model;
mod session;

use session::InnerSession;

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum SpeechError {
    #[error("microphone permission was denied")]
    MicrophoneDenied,
    #[error("microphone permission is restricted on this device")]
    MicrophoneRestricted,
    #[error("speech recognition produced an empty transcript")]
    EmptyTranscript,
    #[error("speech session is not recording")]
    NotRecording,
    #[error("speech session already finished")]
    AlreadyFinished,
    #[error("model not found at {0}")]
    ModelNotFound(String),
    #[error("recognizer failed: {0}")]
    Recognizer(String),
    #[error("audio engine failed: {0}")]
    Audio(String),
}

pub struct SherpaProvider {
    model_dir: std::path::PathBuf,
}

pub struct SpeechSession {
    /// Receive live partial transcription text (may arrive multiple times per second).
    pub partials: std::sync::mpsc::Receiver<String>,
    inner: InnerSession,
}

impl SherpaProvider {
    pub fn new(model_dir: &std::path::Path) -> Result<Self, SpeechError> {
        if !model_dir.exists() {
            return Err(SpeechError::ModelNotFound(model_dir.display().to_string()));
        }
        Ok(Self { model_dir: model_dir.to_owned() })
    }

    pub fn from_env_or_default() -> Result<Self, SpeechError> {
        let dir = model::resolve_model_dir()?;
        Self::new(&dir)
    }

    pub fn start_recording(&self) -> Result<SpeechSession, SpeechError> {
        session::start(self.model_dir.clone())
    }
}

impl SpeechSession {
    pub fn finish(self) -> Result<String, SpeechError> {
        self.inner.finish()
    }

    pub fn cancel(self) {
        self.inner.cancel();
    }
}
```

- [ ] **Step 2: Create `Cargo.toml`**

```toml
[package]
name = "voice_sherpa"
edition.workspace = true
version.workspace = true
license.workspace = true

[dependencies]
thiserror.workspace = true
crossbeam-channel.workspace = true

[target.'cfg(target_os = "macos")'.dependencies]
block2 = "0.6"
objc2 = "0.6"
objc2-av-foundation = { version = "0.3.2", features = ["AVCaptureDevice", "AVMediaFormat", "block2"] }
objc2-avf-audio = { version = "0.3.2", features = ["AVAudioBuffer", "AVAudioConverter", "AVAudioEngine", "AVAudioFormat", "AVAudioIONode", "AVAudioNode", "AVAudioTime", "AVAudioTypes", "block2"] }
objc2-core-foundation = { version = "0.3.2", features = ["CFDate", "CFRunLoop"] }
objc2-foundation = { version = "0.3.2", features = ["NSError", "NSObject", "NSString"] }
sherpa-onnx = "1.13"
```

- [ ] **Step 3: Add stub modules so it compiles**

Create `crates/voice_sherpa/src/permissions.rs` (stub):
```rust
// Real implementation in Task 3.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MicrophoneAuthorization { NotDetermined, Denied, Restricted, Authorized }

pub fn microphone_authorization_from_raw(raw: i64) -> Option<MicrophoneAuthorization> {
    match raw {
        0 => Some(MicrophoneAuthorization::NotDetermined),
        1 => Some(MicrophoneAuthorization::Restricted),
        2 => Some(MicrophoneAuthorization::Denied),
        3 => Some(MicrophoneAuthorization::Authorized),
        _ => None,
    }
}
```

Create `crates/voice_sherpa/src/session_state.rs` (stub, copy from voice_macos — same logic):
```rust
use crate::SpeechError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionPhase { Idle, Recording, Finished, Cancelled }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionEvent { Start, Finish, Cancel }

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SessionState { phase: SessionPhase }

impl Default for SessionPhase { fn default() -> Self { Self::Idle } }

impl SessionState {
    pub fn phase(&self) -> SessionPhase { self.phase }
    pub fn apply(&mut self, event: SessionEvent) -> Result<(), SpeechError> {
        match (self.phase, event) {
            (SessionPhase::Idle, SessionEvent::Start) => { self.phase = SessionPhase::Recording; Ok(()) }
            (SessionPhase::Recording, SessionEvent::Finish) => { self.phase = SessionPhase::Finished; Ok(()) }
            (SessionPhase::Recording, SessionEvent::Cancel) => { self.phase = SessionPhase::Cancelled; Ok(()) }
            (SessionPhase::Finished, SessionEvent::Finish) => Err(SpeechError::AlreadyFinished),
            _ => Err(SpeechError::NotRecording),
        }
    }
}
```

Create `crates/voice_sherpa/src/model.rs` (stub):
```rust
use crate::SpeechError;
use std::path::PathBuf;

pub fn resolve_model_dir() -> Result<PathBuf, SpeechError> {
    if let Ok(path) = std::env::var("MULTISOUL_PARAFORMER_MODEL_DIR") {
        return Ok(PathBuf::from(path));
    }
    // Dev: repo root assets/models/paraformer/
    if let Ok(exe) = std::env::current_exe() {
        let repo_root = exe.ancestors().find(|p| p.join("Cargo.toml").exists());
        if let Some(root) = repo_root {
            let dev_path = root.join("assets/models/paraformer");
            if dev_path.exists() {
                return Ok(dev_path);
            }
        }
    }
    // Release: Contents/Resources/models/paraformer/
    if let Ok(exe) = std::env::current_exe() {
        let bundle_path = exe
            .parent() // MacOS/
            .and_then(|p| p.parent()) // Contents/
            .map(|p| p.join("Resources/models/paraformer"));
        if let Some(path) = bundle_path {
            if path.exists() {
                return Ok(path);
            }
        }
    }
    Err(SpeechError::ModelNotFound(
        "no paraformer model found; run scripts/download-paraformer-model.sh".to_string(),
    ))
}
```

Create `crates/voice_sherpa/src/audio.rs` (stub, real impl in Task 4):
```rust
use crate::SpeechError;

pub(crate) struct AudioCapture;
impl AudioCapture {
    pub(crate) fn start(_sender: crossbeam_channel::Sender<Vec<f32>>) -> Result<Self, SpeechError> {
        unimplemented!("audio capture stub")
    }
    pub(crate) fn stop(self) {}
}
```

Create `crates/voice_sherpa/src/session.rs` (stub, real impl in Task 5):
```rust
use crate::SpeechError;
use std::path::PathBuf;

pub(crate) struct InnerSession;
impl InnerSession {
    pub(crate) fn finish(self) -> Result<String, SpeechError> { unimplemented!("session stub") }
    pub(crate) fn cancel(self) {}
}

pub(crate) fn start(_model_dir: PathBuf) -> Result<crate::SpeechSession, SpeechError> {
    unimplemented!("session start stub")
}
```

- [ ] **Step 4: Register crate in workspace `Cargo.toml`**

In root `Cargo.toml`, add to `members`:
```toml
"crates/voice_sherpa",
```

Add to `[workspace.dependencies]`:
```toml
voice_sherpa = { path = "crates/voice_sherpa" }
```

- [ ] **Step 5: Verify crate compiles**

```bash
cargo check -p voice_sherpa
```

Expected: no errors (stubs compile, unimplemented!() is fine at check time).

- [ ] **Step 6: Commit**

```bash
git add crates/voice_sherpa/ Cargo.toml
git commit -m "feat(voice_sherpa): bootstrap crate with stub modules"
```

---

## Task 3: Microphone permissions

**Files:**
- Create: `crates/voice_sherpa/src/permissions.rs` (replace stub)
- Create: `crates/voice_sherpa/tests/unit/permissions_tests.rs`

**Interfaces:**
- Consumes: `objc2-av-foundation` (AVCaptureDevice), `objc2-core-foundation` (CFRunLoop)
- Produces:
  - `ensure_microphone_authorized() -> Result<(), SpeechError>` (called by session::start)
  - `MicrophoneAuthorization` enum

- [ ] **Step 1: Write failing tests**

Create `crates/voice_sherpa/tests/unit/permissions_tests.rs`:

```rust
use voice_sherpa::permissions::{microphone_authorization_from_raw, MicrophoneAuthorization};

/// PERM-1: Raw AVAuthorizationStatus 값 → MicrophoneAuthorization 매핑 검증
///
/// 데이터 구조 (Apple AVAuthorizationStatus raw 값):
///   0 = NotDetermined, 1 = Restricted, 2 = Denied, 3 = Authorized, 99 = unknown
///
/// 예상 결과:
///   - 0,1,2,3 은 Some(variant)
///   - 99 는 None
#[test]
fn test_microphone_authorization_from_raw_maps_all_variants() {
    assert_eq!(
        microphone_authorization_from_raw(0),
        Some(MicrophoneAuthorization::NotDetermined),
        "raw 0 should be NotDetermined"
    );
    assert_eq!(
        microphone_authorization_from_raw(1),
        Some(MicrophoneAuthorization::Restricted),
        "raw 1 should be Restricted"
    );
    assert_eq!(
        microphone_authorization_from_raw(2),
        Some(MicrophoneAuthorization::Denied),
        "raw 2 should be Denied"
    );
    assert_eq!(
        microphone_authorization_from_raw(3),
        Some(MicrophoneAuthorization::Authorized),
        "raw 3 should be Authorized"
    );
    assert_eq!(
        microphone_authorization_from_raw(99),
        None,
        "raw 99 should be unrecognized"
    );
}
```

- [ ] **Step 2: Run test to verify it fails (or verify stub passes)**

```bash
cargo test -p voice_sherpa permissions_tests
```

Expected: PASS (pure enum mapping, no platform dependency needed).

- [ ] **Step 3: Implement real `permissions.rs`**

Replace stub in `crates/voice_sherpa/src/permissions.rs`:

```rust
use crate::SpeechError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MicrophoneAuthorization {
    NotDetermined,
    Denied,
    Restricted,
    Authorized,
}

pub fn microphone_authorization_from_raw(raw: i64) -> Option<MicrophoneAuthorization> {
    match raw {
        0 => Some(MicrophoneAuthorization::NotDetermined),
        1 => Some(MicrophoneAuthorization::Restricted),
        2 => Some(MicrophoneAuthorization::Denied),
        3 => Some(MicrophoneAuthorization::Authorized),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn ensure_microphone_authorized() -> Result<(), SpeechError> {
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    let media_type = unsafe { AVMediaTypeAudio }
        .ok_or_else(|| SpeechError::Audio("audio media type unavailable".to_string()))?;
    let status = microphone_authorization_from_raw(unsafe {
        AVCaptureDevice::authorizationStatusForMediaType(media_type).0 as i64
    })
    .ok_or_else(|| SpeechError::Audio("unknown microphone authorization status".to_string()))?;
    match status {
        MicrophoneAuthorization::Authorized => Ok(()),
        MicrophoneAuthorization::Denied => Err(SpeechError::MicrophoneDenied),
        MicrophoneAuthorization::Restricted => Err(SpeechError::MicrophoneRestricted),
        MicrophoneAuthorization::NotDetermined => request_microphone_authorization(media_type),
    }
}

#[cfg(target_os = "macos")]
fn request_microphone_authorization(
    media_type: &'static objc2_av_foundation::AVMediaType,
) -> Result<(), SpeechError> {
    use block2::RcBlock;
    use objc2_av_foundation::AVCaptureDevice;
    use objc2_core_foundation::{kCFRunLoopDefaultMode, CFRunLoop};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let (tx, rx) = mpsc::channel::<bool>();
    let block = RcBlock::new(move |granted: objc2::runtime::Bool| {
        let _ = tx.send(granted.as_bool());
    });
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &block) };

    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match rx.try_recv() {
            Ok(true) => return Ok(()),
            Ok(false) => return Err(SpeechError::MicrophoneDenied),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(SpeechError::Audio("mic auth callback closed".to_string()))
            }
            Err(mpsc::TryRecvError::Empty) if Instant::now() >= deadline => {
                return Err(SpeechError::Audio("microphone authorization timed out".to_string()))
            }
            Err(mpsc::TryRecvError::Empty) => {
                if let Some(mode) = unsafe { kCFRunLoopDefaultMode } {
                    CFRunLoop::run_in_mode(Some(mode), 0.02, true);
                } else {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn ensure_microphone_authorized() -> Result<(), SpeechError> {
    Err(SpeechError::Audio("microphone not supported on this platform".to_string()))
}
```

- [ ] **Step 4: Run tests again**

```bash
cargo test -p voice_sherpa permissions_tests
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/voice_sherpa/src/permissions.rs crates/voice_sherpa/tests/
git commit -m "feat(voice_sherpa): microphone permission check and request"
```

---

## Task 4: Audio capture (AVAudioEngine → 16kHz f32 PCM channel)

**Files:**
- Create: `crates/voice_sherpa/src/audio.rs` (replace stub)
- Create: `crates/voice_sherpa/tests/unit/audio_tests.rs`

**Interfaces:**
- Consumes: `crossbeam_channel::Sender<Vec<f32>>`, `objc2-avf-audio`
- Produces:
  - `AudioCapture::start(sender: Sender<Vec<f32>>) -> Result<AudioCapture, SpeechError>`
  - `AudioCapture::stop(self)`

The tap installs at 16kHz mono f32 (Paraformer's required format) so AVAudioEngine handles resampling from the hardware rate.

- [ ] **Step 1: Write test**

Create `crates/voice_sherpa/tests/unit/audio_tests.rs`:

```rust
/// AUDIO-1: AudioCapture::start を呼び出すと PCM データが送信されることを確認。
///
/// 注意: この테스트는 실제 마이크 하드웨어에 의존하므로 CI에서는 건너뜀.
/// 수동 검증용: 실제 마이크가 있는 머신에서 실행.
///
/// 실행 과정:
///   1. crossbeam 채널 생성
///   2. AudioCapture::start(sender) 호출
///   3. 300ms 대기
///   4. AudioCapture::stop() 호출
///   5. 채널에서 적어도 1개 이상의 PCM 청크 수신 확인
///
/// 예상 결과:
///   - start()가 Ok 반환 (마이크 권한 있는 경우)
///   - 채널에서 Vec<f32>가 수신됨
///   - 각 청크는 16kHz mono → 길이가 1600의 배수 (100ms 단위)
#[test]
#[ignore = "requires microphone hardware and permission"]
fn test_audio_capture_sends_pcm_chunks() {
    use crossbeam_channel::unbounded;
    use voice_sherpa::audio_capture_for_test;
    use std::time::Duration;

    let (tx, rx) = unbounded();
    let capture = audio_capture_for_test(tx).expect("AudioCapture::start should succeed with mic permission");
    std::thread::sleep(Duration::from_millis(300));
    capture.stop();

    let mut chunks_received = 0usize;
    while rx.try_recv().is_ok() {
        chunks_received += 1;
    }
    assert!(chunks_received >= 1, "should receive at least one PCM chunk in 300ms");
}
```

- [ ] **Step 2: Implement `audio.rs`**

Replace stub in `crates/voice_sherpa/src/audio.rs`:

```rust
use crate::SpeechError;
use crossbeam_channel::Sender;

const INPUT_BUS: usize = 0;
const TAP_BUFFER_SIZE: u32 = 1_600; // 100ms at 16kHz
const SAMPLE_RATE: f64 = 16_000.0;

pub(crate) struct AudioCapture {
    #[cfg(target_os = "macos")]
    inner: macos::MacosAudioCapture,
}

impl AudioCapture {
    pub(crate) fn start(sender: Sender<Vec<f32>>) -> Result<Self, SpeechError> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self { inner: macos::MacosAudioCapture::start(sender)? })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = sender;
            Err(SpeechError::Audio("audio not supported on this platform".to_string()))
        }
    }

    pub(crate) fn stop(self) {
        #[cfg(target_os = "macos")]
        self.inner.stop();
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2_avf_audio::{
        AVAudioEngine, AVAudioFormat, AVAudioInputNode, AVAudioPCMBuffer, AVAudioTime,
    };
    use std::ptr::NonNull;

    type TapBlock = RcBlock<dyn Fn(NonNull<AVAudioPCMBuffer>, NonNull<AVAudioTime>)>;

    pub(super) struct MacosAudioCapture {
        engine: Retained<AVAudioEngine>,
        input_node: Retained<AVAudioInputNode>,
        _tap_block: TapBlock,
    }

    impl MacosAudioCapture {
        pub(super) fn start(sender: Sender<Vec<f32>>) -> Result<Self, SpeechError> {
            let engine = unsafe { AVAudioEngine::new() };
            let input_node = unsafe { engine.inputNode() };

            // Request 16kHz mono f32 so AVAudioEngine resamples from hardware rate.
            let format = unsafe {
                AVAudioFormat::initStandardFormatWithSampleRate_channels(
                    AVAudioFormat::alloc(),
                    SAMPLE_RATE,
                    1,
                )
            }
            .ok_or_else(|| SpeechError::Audio("could not create 16kHz audio format".to_string()))?;

            let tap_block = RcBlock::new(
                move |buffer: NonNull<AVAudioPCMBuffer>, _: NonNull<AVAudioTime>| {
                    let buf = unsafe { buffer.as_ref() };
                    let frame_count = unsafe { buf.frameLength() } as usize;
                    if frame_count == 0 {
                        return;
                    }
                    // Extract f32 channel data from float channel data pointer.
                    let samples: Vec<f32> = unsafe {
                        let channels = buf.floatChannelData();
                        if channels.is_null() {
                            return;
                        }
                        let channel_ptr = *channels;
                        std::slice::from_raw_parts(channel_ptr, frame_count).to_vec()
                    };
                    let _ = sender.try_send(samples);
                },
            );

            unsafe {
                input_node.installTapOnBus_bufferSize_format_block(
                    INPUT_BUS,
                    TAP_BUFFER_SIZE,
                    Some(&format),
                    RcBlock::as_ptr(&tap_block),
                );
                engine.prepare();
                engine
                    .startAndReturnError()
                    .map_err(|e| SpeechError::Audio(e.localizedDescription().to_string()))?;
            }

            Ok(Self { engine, input_node, _tap_block: tap_block })
        }

        pub(super) fn stop(self) {
            unsafe {
                self.input_node.removeTapOnBus(INPUT_BUS);
                self.engine.stop();
            }
        }
    }
}

/// Exposed for integration testing only (feature-gated if needed).
#[cfg(test)]
pub fn audio_capture_for_test(sender: crossbeam_channel::Sender<Vec<f32>>) -> Result<AudioCapture, SpeechError> {
    AudioCapture::start(sender)
}
```

- [ ] **Step 3: Run compile check**

```bash
cargo check -p voice_sherpa
```

Expected: no errors.

- [ ] **Step 4: Run tests (unit tests pass, hardware test skipped)**

```bash
cargo test -p voice_sherpa audio_tests
```

Expected: 0 failures (hardware test is `#[ignore]`).

- [ ] **Step 5: Commit**

```bash
git add crates/voice_sherpa/src/audio.rs crates/voice_sherpa/tests/unit/audio_tests.rs
git commit -m "feat(voice_sherpa): AVAudioEngine tap at 16kHz mono PCM"
```

---

## Task 5: Streaming recognizer session

**Files:**
- Create: `crates/voice_sherpa/src/session.rs` (replace stub)
- Create: `crates/voice_sherpa/tests/unit/session_state_tests.rs`

**Interfaces:**
- Consumes:
  - `AudioCapture::start(sender)` (Task 4)
  - `permissions::ensure_microphone_authorized()` (Task 3)
  - `SessionState::apply()` (Task 2)
  - `sherpa_onnx::OnlineRecognizer`, `sherpa_onnx::OnlineRecognizerConfig`, etc.
- Produces:
  - `session::start(model_dir: PathBuf) -> Result<SpeechSession, SpeechError>`
  - `InnerSession::finish(self) -> Result<String, SpeechError>`
  - `InnerSession::cancel(self)`

- [ ] **Step 1: Write session state tests**

Create `crates/voice_sherpa/tests/unit/session_state_tests.rs`:

```rust
use voice_sherpa::session_state::{SessionEvent, SessionPhase, SessionState};
use voice_sherpa::SpeechError;

/// SS-1: 정상 흐름 Idle → Recording → Finished
///
/// 실행 과정:
///   1. Start 이벤트 → Idle → Recording
///   2. Finish 이벤트 → Recording → Finished
///
/// 예상 결과:
///   - 두 transition 모두 Ok
///   - 최종 phase == Finished
#[test]
fn test_session_state_normal_flow() {
    let mut state = SessionState::default();
    assert_eq!(state.apply(SessionEvent::Start), Ok(()), "Start from Idle should succeed");
    assert_eq!(state.phase(), SessionPhase::Recording, "should be Recording after Start");
    assert_eq!(state.apply(SessionEvent::Finish), Ok(()), "Finish from Recording should succeed");
    assert_eq!(state.phase(), SessionPhase::Finished, "should be Finished after Finish");
}

/// SS-2: Finished 상태에서 Finish 재시도 → AlreadyFinished
///
/// 예상 결과:
///   - 두 번째 Finish는 Err(AlreadyFinished)
#[test]
fn test_session_state_double_finish_is_error() {
    let mut state = SessionState::default();
    state.apply(SessionEvent::Start).unwrap();
    state.apply(SessionEvent::Finish).unwrap();
    assert_eq!(
        state.apply(SessionEvent::Finish),
        Err(SpeechError::AlreadyFinished),
        "double finish should return AlreadyFinished"
    );
}

/// SS-3: Idle 상태에서 Finish → NotRecording
///
/// 예상 결과:
///   - Err(NotRecording)
#[test]
fn test_session_state_finish_without_start_is_error() {
    let mut state = SessionState::default();
    assert_eq!(
        state.apply(SessionEvent::Finish),
        Err(SpeechError::NotRecording),
        "finish without start should return NotRecording"
    );
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p voice_sherpa session_state_tests
```

Expected: PASS (session_state logic was in stub already).

- [ ] **Step 3: Implement `session.rs`**

Replace stub in `crates/voice_sherpa/src/session.rs`:

```rust
use crate::audio::AudioCapture;
use crate::permissions::ensure_microphone_authorized;
use crate::session_state::{SessionEvent, SessionState};
use crate::SpeechError;
use crossbeam_channel::unbounded;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const RECOGNIZER_THREAD_DRAIN_TIMEOUT: Duration = Duration::from_millis(1_500);

pub(crate) struct InnerSession {
    audio: Option<AudioCapture>,
    state: Arc<Mutex<SessionState>>,
    thread_handle: Option<std::thread::JoinHandle<String>>,
    stop_tx: crossbeam_channel::Sender<()>,
}

impl InnerSession {
    pub(crate) fn finish(mut self) -> Result<String, SpeechError> {
        {
            let mut state = self.state.lock().map_err(|_| SpeechError::Recognizer("state lock poisoned".to_string()))?;
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
            Some(handle) => handle.join().map_err(|_| SpeechError::Recognizer("recognizer thread panicked".to_string())),
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

fn build_recognizer_config(model_dir: &PathBuf) -> Result<sherpa_onnx::OnlineRecognizerConfig, SpeechError> {
    use sherpa_onnx::*;

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
                encoder: encoder.to_string_lossy().into_owned(),
                decoder: decoder.to_string_lossy().into_owned(),
            },
            tokens: tokens.to_string_lossy().into_owned(),
            num_threads: 2,
            debug: 0,
            provider: "cpu".to_string(),
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
    let recognizer = match sherpa_onnx::OnlineRecognizer::new(&config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("voice_sherpa: failed to create recognizer: {e}");
            return String::new();
        }
    };
    let mut stream = recognizer.create_stream();
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
                recognizer.decode(&mut stream);
            }
            let text = recognizer.get_result(&stream).text.trim().to_string();
            if !text.is_empty() && text != last_text {
                last_text = text.clone();
                let _ = partial_tx.send(text);
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
    while recognizer.is_ready(&stream) {
        recognizer.decode(&mut stream);
    }
    let final_text = recognizer.get_result(&stream).text.trim().to_string();
    if !final_text.is_empty() {
        let _ = partial_tx.send(final_text.clone());
    }

    final_text
}
```

- [ ] **Step 4: Run compile check**

```bash
cargo check -p voice_sherpa
```

Expected: no errors. (If `sherpa_onnx::OnlineParaformerModelConfig` doesn't exist at this exact name, check `cargo doc -p sherpa-onnx --open` and adjust field names to match the actual 1.13.x API.)

- [ ] **Step 5: Run all voice_sherpa tests**

```bash
cargo test -p voice_sherpa
```

Expected: session state tests PASS; hardware tests SKIP.

- [ ] **Step 6: Commit**

```bash
git add crates/voice_sherpa/src/session.rs crates/voice_sherpa/tests/unit/session_state_tests.rs
git commit -m "feat(voice_sherpa): streaming recognizer session with partials channel"
```

---

## Task 6: Model path resolution test

**Files:**
- Create: `crates/voice_sherpa/tests/unit/model_tests.rs`
- Modify: `crates/voice_sherpa/src/model.rs` (verify final logic)

**Interfaces:**
- Consumes: `MULTISOUL_PARAFORMER_MODEL_DIR` env var
- Produces: `resolve_model_dir() -> Result<PathBuf, SpeechError>` (finalized)

- [ ] **Step 1: Write failing test**

Create `crates/voice_sherpa/tests/unit/model_tests.rs`:

```rust
use voice_sherpa::SpeechError;

/// MODEL-1: 환경 변수 설정 시 그 경로를 반환해야 함
///
/// 실행 과정:
///   1. MULTISOUL_PARAFORMER_MODEL_DIR=/tmp/test-model 설정
///   2. resolve_model_dir() 호출 (디렉토리 없어도 env var 경로 반환)
///
/// 예상 결과:
///   - Ok(PathBuf::from("/tmp/test-model"))
#[test]
fn test_resolve_model_dir_from_env() {
    // Note: resolve_model_dir checks existence only when env is NOT set.
    // When env var is present it trusts the value.
    std::env::set_var("MULTISOUL_PARAFORMER_MODEL_DIR", "/tmp/test-model-dir");
    let result = voice_sherpa::resolve_model_dir_for_test();
    std::env::remove_var("MULTISOUL_PARAFORMER_MODEL_DIR");
    assert_eq!(
        result,
        Ok(std::path::PathBuf::from("/tmp/test-model-dir")),
        "should return env var path without checking existence"
    );
}
```

Expose `resolve_model_dir_for_test` in `lib.rs` (test-only re-export):
```rust
#[cfg(test)]
pub fn resolve_model_dir_for_test() -> Result<std::path::PathBuf, SpeechError> {
    model::resolve_model_dir()
}
```

- [ ] **Step 2: Run test**

```bash
cargo test -p voice_sherpa model_tests
```

Expected: PASS (env var branch is already in the stub).

- [ ] **Step 3: Commit**

```bash
git add crates/voice_sherpa/tests/unit/model_tests.rs crates/voice_sherpa/src/lib.rs
git commit -m "test(voice_sherpa): model path resolution test"
```

---

## Task 7: Wire into app layer

**Files:**
- Modify: `crates/app/Cargo.toml`
- Modify: `crates/app/src/app/voice/control.rs`
- Modify: `crates/app/src/app/voice/mod.rs`
- Modify: `Cargo.toml` (workspace)

**Interfaces:**
- Consumes: `voice_sherpa::SherpaProvider`, `voice_sherpa::SpeechSession`, `voice_sherpa::SpeechError`
- Produces: compiling `app` crate; `voice_label()` shows live partial text during recording

- [ ] **Step 1: Update workspace `Cargo.toml`**

In root `Cargo.toml`:
1. In `members`, **remove** `"crates/voice_macos"`, keep `"crates/voice_sherpa"` (already added in Task 2).
2. In `[workspace.dependencies]`, **remove** `voice_macos = ...`.

- [ ] **Step 2: Update `crates/app/Cargo.toml`**

Replace:
```toml
voice_macos.workspace = true
```
With:
```toml
voice_sherpa.workspace = true
```

- [ ] **Step 3: Update `control.rs`**

In `crates/app/src/app/voice/control.rs`:

1. Change `use` at line 25 from `voice_macos::SpeechSession` to `voice_sherpa::SpeechSession`.
2. Add `partial_text: String` field to `VoiceControlState`.
3. Add `set_partial_text` and `take_partials_receiver` methods.

Full diff for the struct and impl block (replace lines 23-37 and 113-137):

```rust
pub(crate) struct VoiceControlState {
    phase: VoicePhase,
    speech_session: Option<voice_sherpa::SpeechSession>,
    speech_task: Option<Task<()>>,
    pub(crate) partial_text: String,
}

impl Default for VoiceControlState {
    fn default() -> Self {
        Self {
            phase: VoicePhase::Idle,
            speech_session: None,
            speech_task: None,
            partial_text: String::new(),
        }
    }
}
```

Replace the two methods that referenced `voice_macos`:
```rust
pub(crate) fn set_speech_session(&mut self, session: voice_sherpa::SpeechSession) {
    self.speech_session = Some(session);
}

pub(crate) fn take_speech_session(&mut self) -> Option<voice_sherpa::SpeechSession> {
    self.speech_session.take()
}

pub(crate) fn cancel_speech_session(&mut self) {
    if let Some(session) = self.speech_session.take() {
        session.cancel();
    }
}
```

Update `label()` to show live partials during recording:
```rust
pub(crate) fn label(&self) -> String {
    match &self.phase {
        VoicePhase::Idle => "Hold Cmd+Shift+V".to_string(),
        VoicePhase::Recording { .. } => {
            if self.partial_text.is_empty() {
                "Recording...".to_string()
            } else {
                self.partial_text.clone()
            }
        }
        VoicePhase::Transcribing => "Transcribing...".to_string(),
        VoicePhase::Planning { .. } => "Planning...".to_string(),
        VoicePhase::Confirming { .. } => "Confirmation required".to_string(),
        VoicePhase::Executed { message } => message.clone(),
        VoicePhase::TranscriptOnly { transcript, .. } => transcript.clone(),
        VoicePhase::Error { message } => message.clone(),
    }
}
```

- [ ] **Step 4: Update `mod.rs`**

In `crates/app/src/app/voice/mod.rs`:

Replace:
```rust
use voice_macos::{AppleSpeechProvider, SpeechError};
```
With:
```rust
use voice_sherpa::{SherpaProvider, SpeechError};
```

Replace `begin_voice_recording` body (the `AppleSpeechProvider::start_recording` call):
```rust
if matches!(voice_transcript_source(), VoiceTranscriptSource::AppleSpeech) {
    match SherpaProvider::from_env_or_default().and_then(|p| p.start_recording()) {
        Ok(session) => self.voice.set_speech_session(session),
        Err(error) => {
            self.voice.set_error(format!("Speech recognition failed: {error}"));
            self.save_voice_speech_error(error);
        }
    }
}
```

Add partial draining in `process_apple_speech_recording`. Before spawning the async task, add a GPUI-compatible periodic callback. Since GPUI apps render on demand, the simplest approach is to drain partials inside the async task's loop using `cx.notify()`:

```rust
fn process_apple_speech_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(session) = self.voice.take_speech_session() else {
        let error = SpeechError::NotRecording;
        self.voice.set_error(format!("Speech recognition failed: {error}"));
        self.save_voice_speech_error(error);
        return;
    };
    // Drain any already-queued partials immediately.
    while let Ok(text) = session.partials.try_recv() {
        self.voice.partial_text = text;
    }
    cx.notify();
    let task = cx.spawn_in(window, async move |this, cx| {
        // Poll partials while waiting for finish().
        // finish() blocks until the recognizer thread exits.
        // We spawn finish on a blocking thread and poll partials on a timer.
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let partials = session.partials; // moved into polling side
        let session_for_finish = /* see note below */;
        // NOTE: SpeechSession::finish takes self. We need to split partials from inner.
        // This is handled by the design: partials is a pub field separate from inner.
        // Spawn blocking finish:
        let result = cx.background_executor().spawn(async move {
            session.finish()
        }).await;
        let _ = this.update_in(cx, |app, _window, cx| {
            app.voice.clear_speech_task();
            app.voice.partial_text.clear();
            app.handle_sherpa_speech_result(result, cx);
        });
    });
    self.voice.set_speech_task(task);
}
```

**Important:** `SpeechSession::partials` is a `std::sync::mpsc::Receiver<String>`, which is not `Sync`. To poll it while `finish()` runs on a background thread, split: move `session` (which contains `partials`) into the background executor closure, and have `finish()` return the final string. Since `partials` is drained by the recognizer thread pushing into it until stop, the final text from `finish()` already contains the full transcript. The partial drain just updates the UI label during recording.

Rename the existing `process_apple_speech_recording` and `handle_apple_speech_result` in place:

```rust
fn process_sherpa_speech_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(session) = self.voice.take_speech_session() else {
        let error = SpeechError::NotRecording;
        self.voice.set_error(format!("Speech recognition failed: {error}"));
        self.save_voice_speech_error(error);
        return;
    };
    // Drain any partials already queued before key-up.
    while let Ok(text) = session.partials.try_recv() {
        self.voice.partial_text = text;
    }
    cx.notify();
    // finish() blocks (recognizer thread join); run in spawn_in to match existing pattern.
    let task = cx.spawn_in(window, async move |this, cx| {
        let result = session.finish();
        let _ = this.update_in(cx, |app, window, cx| {
            app.voice.clear_speech_task();
            app.voice.partial_text.clear();
            app.handle_sherpa_speech_result(result, window, cx);
        });
    });
    self.voice.set_speech_task(task);
}

fn handle_sherpa_speech_result(
    &mut self,
    result: Result<String, SpeechError>,
    window: &mut Window,
    cx: &mut Context<Self>,
) {
    match result {
        Ok(transcript) if !transcript.trim().is_empty() => {
            self.handle_voice_transcript(transcript.trim().to_string(), window, cx);
        }
        Ok(_) | Err(SpeechError::EmptyTranscript) => {
            self.record_empty_voice_transcript();
        }
        Err(error) => {
            self.voice.set_error(format!("Speech recognition failed: {error}"));
            self.save_voice_speech_error(error);
        }
    }
}
```

Also rename the call site in `process_finished_voice_recording`:
```rust
VoiceTranscriptSource::AppleSpeech => {
    self.process_sherpa_speech_recording(window, cx);
}
```

Also update `save_voice_speech_error` to accept `voice_sherpa::SpeechError` (same signature, just type alias change). Update the provider string in history saves: `Some("sherpa_onnx_paraformer".to_string())`.

- [ ] **Step 5: Check app compiles**

```bash
cargo check -p app
```

Fix any type errors (the `window` parameter pattern, `SpeechError` variant mismatches). The key variants to check:
- `SpeechError::SpeechDenied` → `SpeechError::MicrophoneDenied` (no speech auth in sherpa)
- `SpeechError::SpeechRestricted` → `SpeechError::MicrophoneRestricted`
- `SpeechError::RecognizerUnavailable` → `SpeechError::Recognizer(_)` or remove that branch
- Other `SpeechError` variants have different names — map to equivalent display strings.

- [ ] **Step 6: Run workspace tests**

```bash
cargo test --workspace --exclude voice_macos
```

Expected: all existing tests pass; new voice_sherpa tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/app/Cargo.toml crates/app/src/app/voice/ Cargo.toml
git commit -m "feat(app): wire voice_sherpa streaming provider into app layer"
```

---

## Task 8: Delete `voice_macos` crate

**Files:**
- Delete: `crates/voice_macos/` (entire directory)
- Verify: workspace `Cargo.toml` no longer references it

**Interfaces:**
- Consumes: Task 7 complete (app no longer imports voice_macos)

- [ ] **Step 1: Verify no remaining references**

```bash
grep -r "voice_macos" crates/ Cargo.toml
```

Expected: no output. If any references remain, fix them before deleting.

- [ ] **Step 2: Delete the crate directory**

```bash
rm -rf crates/voice_macos
```

- [ ] **Step 3: Verify workspace still builds**

```bash
cargo check --workspace
cargo test --workspace
```

Expected: no errors, no test failures.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: remove voice_macos crate (replaced by voice_sherpa)"
```

---

## Task 9: End-to-end smoke test

**Files:**
- Read: no file changes — this is a manual verification task

**Interfaces:**
- Consumes: all previous tasks complete, model files present in `assets/models/paraformer/`

- [ ] **Step 1: Verify model files present**

```bash
ls -lh assets/models/paraformer/
```

Expected: `encoder.int8.onnx` (or `encoder.onnx`), `decoder.int8.onnx` (or `decoder.onnx`), `tokens.txt`.

- [ ] **Step 2: Run the app**

```bash
cargo run -p app
```

Expected: app launches without errors.

- [ ] **Step 3: Test push-to-talk**

1. Hold `Cmd+Shift+V` and say a sentence in Chinese (e.g., "创建一个新的终端节点").
2. While holding, the bottom bar label should update with live partial text.
3. Release the keys.
4. The bottom bar should show "Transcribing..." briefly, then the planning result.

Expected: < 500ms from release to final transcript appearing.

- [ ] **Step 4: Test boundary — no model files**

Temporarily rename `assets/models/paraformer/` and restart:
```bash
mv assets/models/paraformer assets/models/paraformer.bak
cargo run -p app
# Hold Cmd+Shift+V
# Expected: bottom bar shows "Speech recognition failed: model not found..."
mv assets/models/paraformer.bak assets/models/paraformer
```

- [ ] **Step 5: Verify dependency boundary**

```bash
cargo tree -p voice_sherpa | grep -i gpui
```

Expected: no output (GPUI-free domain crate constraint).

- [ ] **Step 6: Run constraint checks**

```bash
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
```

Expected: all pass.

- [ ] **Step 7: Final commit**

```bash
git add -A
git commit -m "feat: complete voice_sherpa streaming STT integration"
```
