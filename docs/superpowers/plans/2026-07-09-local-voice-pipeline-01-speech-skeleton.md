# Local Voice Pipeline 01 — Speech Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract neutral PCM types, introduce dialogue client traits + session state machine, delete OpenAI Realtime/sidecar, and deliver `Mic → VAD → ASR` plus whole-utterance TTS with barge-in stop.

**Architecture:** Add a tiny GPUI-free `crates/audio` for PCM. Add `crates/dialogue` for `VadClient` / `AsrClient` / `TtsClient` traits and a pure `DialogueSession` state machine (`Idle/Listening/Thinking/Speaking`). Keep sherpa adapters in `voice_sherpa`. Replace app `realtime/*` Always Listening shell with a dialogue-backed shell that does not start a sidecar. Do not implement real `LlmClient` or card tools in this plan — use stubs so the speech path compiles and tests.

**Tech Stack:** Rust 2021, `sherpa-onnx`, AVAudioEngine (existing `voice_sherpa`), GPUI `=0.2.2` app shell only, `async-channel`, `crossbeam-channel`, `thiserror`, `serde`.

**Spec:** [`docs/product-specs/2026-07-09-SPEC-local-voice-pipeline.md`](../../product-specs/2026-07-09-SPEC-local-voice-pipeline.md) §6.4 steps 1–4.

**Follow-on plan:** `docs/superpowers/plans/2026-07-09-local-voice-pipeline-02-llm-and-card-tools.md`

---

## Global Constraints

- Do **not** delete `crates/realtime` until `voice_sherpa` and `app` playback no longer import `realtime::PcmAudioChunk`.
- `runtime` / `canvas` / `terminal` / `storage` must stay free of sherpa, Ollama, dialogue, and GPUI.
- Existing one-shot voice command path (`voice` planner + `SherpaProvider::start_recording`) must keep compiling and passing tests unless a task explicitly migrates a shared helper.
- Models are **process-resident** after first successful Listening start; closing Listening must not unload ASR/VAD/TTS handles. Window close releases them.
- No `#[allow(...)]`. Rust files ≤ 500 lines. Tests need Chinese scenario comments and assertion messages per `docs/quality/test-requirements.md`.
- Heavy work stays off the GPUI foreground path.
- Prefer TDD: failing test → implement → pass → commit.
- Do not commit unless the user asks; plan commit steps are for the implementing agent when commits are requested.

---

## File Structure

Create:

- `crates/audio/Cargo.toml`
- `crates/audio/src/lib.rs`
- `crates/audio/src/pcm.rs`
- `crates/audio/tests/unit/pcm_tests.rs`
- `crates/dialogue/Cargo.toml`
- `crates/dialogue/src/lib.rs`
- `crates/dialogue/src/clients.rs` — `VadClient`, `AsrClient`, `TtsClient` traits + event types
- `crates/dialogue/src/session.rs` — `DialoguePhase`, `DialogueSession` pure state machine
- `crates/dialogue/src/report_queue.rs` — template TTS queue policy (stub enqueue/dequeue rules)
- `crates/dialogue/tests/unit/session_tests.rs`
- `crates/dialogue/tests/unit/report_queue_tests.rs`
- `crates/voice_sherpa/src/vad.rs` — sherpa VAD adapter (behind trait object or concrete type implementing `dialogue::VadClient`)
- `crates/voice_sherpa/src/tts.rs` — sherpa Offline TTS adapter implementing `dialogue::TtsClient`
- `crates/voice_sherpa/tests/unit/vad_tests.rs`
- `crates/voice_sherpa/tests/unit/tts_tests.rs` (unit with fake samples; ignored demo may already exist)
- `crates/app/src/app/dialogue/` — replace realtime shell wiring
- `crates/app/tests/unit/app_dialogue_shell_tests.rs`

Modify:

- `Cargo.toml` — workspace members + deps for `audio`, `dialogue`
- `crates/voice_sherpa/Cargo.toml` — depend on `audio` + `dialogue`; remove `realtime`
- `crates/voice_sherpa/src/lib.rs`, `audio.rs`, `stream.rs` — use `audio::PcmAudioChunk`
- `crates/voice_sherpa/src/session.rs` — optional shared capture helpers for VAD/ASR path
- `crates/app/Cargo.toml` — depend on `audio`, `dialogue`; remove `realtime`
- `crates/app/src/app/voice/playback.rs` — `audio::PcmAudioChunk`
- `crates/app/src/app/mod.rs` — swap `realtime` module for `dialogue`
- `crates/app/src/settings/voice/**` — remove OpenAI Realtime/sidecar settings; keep Always Listening toggle + device + Ollama placeholders as needed for later plan
- `crates/e2e/Cargo.toml` and realtime e2e — mark obsolete or delete in Task 4
- `Packager.toml` / packaging docs — drop sidecar packaging if present

Delete (only after rewires compile):

- `crates/realtime/**`
- `sidecars/openai-realtime/**`
- `crates/app/src/app/realtime/**`
- Realtime-only app unit tests that cannot be rewritten as dialogue shell tests

Do not modify in this plan:

- `crates/llm` (created in plan 02)
- Full Ollama chat tool-calling
- Flattening `card_task` away from Planner (plan 02)
- `List` / `Delete` tools

---

### Task 1: Neutral `crates/audio` PCM Types

**Files:**
- Create: `crates/audio/Cargo.toml`
- Create: `crates/audio/src/lib.rs`
- Create: `crates/audio/src/pcm.rs`
- Create: `crates/audio/tests/unit/pcm_tests.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `PcmFormat`, `PcmAudioChunk`, `PcmAudioError`
- Note: unlike old realtime (hard 24_000 only), accept **16_000** (ASR/VAD) and **24_000** (playback legacy). Reject other rates in MVP.

- [ ] **Step 1: Write failing PCM tests**

```rust
// crates/audio/tests/unit/pcm_tests.rs
use audio::{PcmAudioChunk, PcmAudioError, PcmFormat};

/// PCM-1: 16k/24k mono PCM16 合法；其它采样率与奇数字节非法。
#[test]
fn test_pcm_accepts_16k_and_24k_rejects_other_rates_and_odd_bytes() {
    assert!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0, 1]).is_ok(),
        "16_000Hz mono PCM16 even bytes must be accepted for ASR/VAD"
    );
    assert!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0, 1, 2, 3]).is_ok(),
        "24_000Hz mono PCM16 must remain accepted for playback chunks"
    );
    assert_eq!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 48_000, 1, vec![0, 1]),
        Err(PcmAudioError::InvalidSampleRate),
        "MVP must reject sample rates other than 16k/24k"
    );
    assert_eq!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 2, vec![0, 1]),
        Err(PcmAudioError::InvalidChannels),
        "MVP audio path is mono-only"
    );
    assert_eq!(
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0]),
        Err(PcmAudioError::InvalidPcm16Length),
        "odd PCM16 byte length must be rejected"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p audio -- --nocapture`  
Expected: FAIL (crate missing or type missing)

- [ ] **Step 3: Implement `crates/audio`**

```rust
// crates/audio/src/pcm.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PcmFormat {
    Pcm16,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PcmAudioChunk {
    pub format: PcmFormat,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmAudioError {
    InvalidSampleRate,
    InvalidChannels,
    InvalidPcm16Length,
}

impl PcmAudioChunk {
    pub fn new(
        format: PcmFormat,
        sample_rate_hz: u32,
        channels: u16,
        bytes: Vec<u8>,
    ) -> Result<Self, PcmAudioError> {
        if sample_rate_hz != 16_000 && sample_rate_hz != 24_000 {
            return Err(PcmAudioError::InvalidSampleRate);
        }
        if channels != 1 {
            return Err(PcmAudioError::InvalidChannels);
        }
        if matches!(format, PcmFormat::Pcm16) && bytes.len() % 2 != 0 {
            return Err(PcmAudioError::InvalidPcm16Length);
        }
        Ok(Self {
            format,
            sample_rate_hz,
            channels,
            bytes,
        })
    }
}
```

Wire `lib.rs` with `mod pcm; pub use pcm::*;` and `#[cfg(test)] #[path = "../tests/unit/pcm_tests.rs"] mod pcm_tests;`.

Add workspace member + `audio = { path = "crates/audio" }`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p audio`  
Expected: PASS

- [ ] **Step 5: Commit** (when user requests)

```bash
git add Cargo.toml crates/audio
git commit -m "$(cat <<'EOF'
Add crates/audio for neutral PCM16 chunk validation.

EOF
)"
```

---

### Task 2: Rewire `voice_sherpa` And App Playback Off `realtime`

**Files:**
- Modify: `crates/voice_sherpa/Cargo.toml`
- Modify: `crates/voice_sherpa/src/lib.rs`
- Modify: `crates/voice_sherpa/src/audio.rs`
- Modify: `crates/voice_sherpa/src/stream.rs`
- Modify: `crates/voice_sherpa/tests/unit/audio_tests.rs`
- Modify: `crates/app/src/app/voice/playback.rs`
- Modify: any app imports of `realtime::PcmAudioChunk`

**Interfaces:**
- `SherpaProvider::start_pcm_stream` returns `Receiver<audio::PcmAudioChunk>`
- Capture for dialogue ASR path should target **16_000** Hz (update stream helper; keep a parameter or separate constant). Old realtime used 24_000 — change default stream rate to 16_000 for VAD/ASR; playback sink still accepts 24_000 if TTS emits that rate.

- [ ] **Step 1: Update failing expectations in `voice_sherpa` audio tests**

Change assertions that require `24_000` for the capture stream to `16_000`, and import `audio::{PcmAudioChunk, PcmFormat}` instead of `realtime`.

- [ ] **Step 2: Run to see failures**

Run: `cargo test -p voice_sherpa audio_tests -- --nocapture`  
Expected: FAIL on rate / missing `audio` dep

- [ ] **Step 3: Minimal rewire**

- `voice_sherpa` Cargo.toml: `audio.workspace = true`; remove `realtime.workspace = true`
- Replace all `realtime::` PCM imports with `audio::`
- Set PCM stream target sample rate to `16_000`
- `playback.rs`: `use audio::PcmAudioChunk`

- [ ] **Step 4: Verify**

Run:

```bash
cargo test -p audio -p voice_sherpa
cargo check -p app
```

Expected: `voice_sherpa` tests PASS; `app` may still depend on `realtime` for shell — check must at least resolve PCM types. If `app` still needs `realtime` for other modules, leave workspace dep until Task 4.

- [ ] **Step 5: Commit** (when requested)

```bash
git add crates/voice_sherpa crates/app/src/app/voice/playback.rs Cargo.toml
git commit -m "$(cat <<'EOF'
Route PCM capture and playback through crates/audio.

EOF
)"
```

---

### Task 3: `crates/dialogue` Client Traits And Session State Machine

**Files:**
- Create: `crates/dialogue/**` as listed above
- Modify: `Cargo.toml`

**Interfaces:**

```rust
// crates/dialogue/src/clients.rs
use audio::PcmAudioChunk;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VadEvent {
    SpeechStart,
    SpeechEnd,
}

pub trait VadClient: Send {
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<VadEvent>, DialogueError>;
    fn reset(&mut self);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AsrEvent {
    Partial(String),
    Final(String),
}

pub trait AsrClient: Send {
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<AsrEvent>, DialogueError>;
    fn end_utterance(&mut self) -> Result<Option<String>, DialogueError>;
    fn reset(&mut self);
}

pub trait TtsClient: Send {
    /// MVP: synthesize a full utterance. Streaming can be added later without changing callers much.
    fn synthesize(&mut self, text: &str) -> Result<PcmAudioChunk, DialogueError>;
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum DialogueError {
    #[error("{0}")]
    Message(String),
}
```

```rust
// crates/dialogue/src/session.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialoguePhase {
    Idle,
    Listening,
    Thinking,
    Speaking,
}

pub struct DialogueSession {
    phase: DialoguePhase,
    // resident flag: models stay loaded across stop_listening
    models_resident: bool,
}

impl DialogueSession {
    pub fn new() -> Self { /* Idle, models_resident=false */ }

    pub fn phase(&self) -> DialoguePhase { self.phase }

    pub fn on_listening_started(&mut self) {
        self.phase = DialoguePhase::Listening;
        self.models_resident = true;
    }

    pub fn on_listening_stopped(&mut self) {
        // SPEC: stop mic/tts/tasks elsewhere; keep models_resident == true
        self.phase = DialoguePhase::Idle;
    }

    pub fn on_window_closed(&mut self) {
        self.phase = DialoguePhase::Idle;
        self.models_resident = false;
    }

    pub fn on_utterance_final(&mut self) {
        assert_eq!(self.phase, DialoguePhase::Listening);
        self.phase = DialoguePhase::Thinking;
    }

    pub fn on_reply_ready(&mut self) {
        assert_eq!(self.phase, DialoguePhase::Thinking);
        self.phase = DialoguePhase::Speaking;
    }

    pub fn on_playback_finished_or_interrupted(&mut self) {
        // return to Listening if session still active — app passes a flag
    }

    pub fn models_resident(&self) -> bool { self.models_resident }
}
```

Report queue (pure):

```rust
pub enum ReportPlayDecision {
    PlayNow,
    Queue,
    DeferWhileUserSpeaking,
}

pub fn decide_template_report(
    phase: DialoguePhase,
    user_speech_active: bool,
) -> ReportPlayDecision {
    if user_speech_active {
        return ReportPlayDecision::DeferWhileUserSpeaking;
    }
    match phase {
        DialoguePhase::Speaking => ReportPlayDecision::Queue,
        DialoguePhase::Listening | DialoguePhase::Idle => ReportPlayDecision::PlayNow,
        DialoguePhase::Thinking => ReportPlayDecision::Queue,
    }
}
```

- [ ] **Step 1: Write session + report queue tests** (Chinese comments, concrete asserts)

Cover:
- start listening → `Listening`, `models_resident=true`
- stop listening → `Idle`, `models_resident` still true
- window closed → `models_resident=false`
- final utterance → `Thinking` → reply ready → `Speaking`
- template report: Speaking ⇒ Queue; user speaking ⇒ Defer; Listening ⇒ PlayNow

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p dialogue -- --nocapture`

- [ ] **Step 3: Implement minimal session + traits**

Keep files under 500 lines; no GPUI.

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p dialogue`

- [ ] **Step 5: Commit** (when requested)

```bash
git add Cargo.toml crates/dialogue
git commit -m "$(cat <<'EOF'
Add dialogue crate with speech client traits and session phases.

EOF
)"
```

---

### Task 4: Remove Realtime Sidecar And Replace Always Listening Shell

**Files:**
- Create: `crates/app/src/app/dialogue/mod.rs`, `shell.rs` (and thin stubs for chat later)
- Modify: `crates/app/src/app/mod.rs`
- Modify: `crates/app/src/settings/voice/**`
- Modify: `crates/app/Cargo.toml`
- Delete: `crates/app/src/app/realtime/**`, realtime-only tests, `crates/realtime/**`, `sidecars/openai-realtime/**`
- Modify: `crates/e2e/**` — remove or `#[ignore]` + comment pointing to new SPEC
- Modify: `Packager.toml` if it packages the sidecar
- Modify: design-doc README to mark realtime MVP superseded by the new SPEC

**Behavior for this task (skeleton only):**

- Settings toggle / bottom bar still starts/stops Always Listening
- Start path: mic permission + PCM stream + `DialogueSession::on_listening_started`
- **No** sidecar process, **no** OpenAI key requirement
- Stop path: stop mic/PCM, `on_listening_stopped`, cancel card tasks if wired (call existing cancel helper if still present; otherwise leave a `todo` hook with a test that the shell invokes a `CardTaskCancel` callback trait object — prefer a no-op callback in this task)
- Bottom bar labels map from `DialoguePhase` (`Listening` / later Thinking/Speaking)
- Errors: mic denied / model missing surface on the bar

- [ ] **Step 1: Add failing app shell unit tests**

Rewrite the smallest useful tests from `app_realtime_tests` / lifecycle tests into `app_dialogue_shell_tests.rs`:

- starting Always Listening does not require OpenAI API key
- stopping returns phase to Idle but a test double reports models still resident
- starting fails cleanly when mic permission helper returns denied (inject seam)

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p app app_dialogue_shell -- --nocapture`

- [ ] **Step 3: Implement shell swap + delete realtime**

Order matters:

1. Add `app/dialogue` shell that compiles alongside realtime briefly if needed  
2. Point `mod.rs` + settings toggle at dialogue shell  
3. Delete realtime modules/tests/sidecar/crate  
4. Remove `realtime` from workspace `Cargo.toml` / `app` / `e2e` deps  
5. `cargo check -p app` and `cargo test -p audio -p dialogue -p voice_sherpa -p app`

- [ ] **Step 4: Verify no sidecar leftovers**

Run:

```bash
rg -n "openai-realtime|RealtimeSidecar|crates/realtime" --glob '!docs/**' --glob '!**/2026-07-09-SPEC*' --glob '!**/2026-07-09-local-voice*'
cargo tree -p voice_sherpa -p dialogue -p audio | rg -i realtime || true
```

Expected: no runtime code references; docs historical mentions OK.

- [ ] **Step 5: Commit** (when requested)

```bash
git add -A
git commit -m "$(cat <<'EOF'
Replace Always Listening realtime sidecar with dialogue shell skeleton.

EOF
)"
```

---

### Task 5: Mic → VAD → ASR Skeleton

**Files:**
- Create/Modify: `crates/voice_sherpa/src/vad.rs`, ASR streaming wrapper (extend `session.rs` or new `asr_stream.rs`)
- Modify: `crates/voice_sherpa/src/lib.rs`, `Cargo.toml` (depend on `dialogue`)
- Modify: `crates/app/src/app/dialogue/` — background loop: PCM → VAD → ASR events → session phase transitions
- Test: `crates/voice_sherpa/tests/unit/vad_tests.rs`, dialogue integration test with fake VAD/ASR

**Behavior:**

- While `Listening`, feed 16 kHz PCM into `VadClient`
- On `SpeechStart`, begin/continue ASR push
- On `SpeechEnd`, `AsrClient::end_utterance` → final transcript
- On final transcript: `DialogueSession::on_utterance_final` → `Thinking`
- This plan’s Thinking stub: immediately clear thinking with empty/no LLM (or set a `pending_transcript` field only). **Do not** call Ollama yet.
- Expose partial transcripts to UI if cheap; otherwise finals-only is OK for MVP skeleton

- [ ] **Step 1: Fake client tests in `dialogue` or `voice_sherpa`**

```rust
/// VAD-ASR-1: SpeechEnd 后 Asr final 会把 session 推到 Thinking。
#[test]
fn test_speech_end_final_transcript_moves_session_to_thinking() {
    // FakeVad emits Start then End
    // FakeAsr returns Final("你好")
    // drive a small pure helper `advance_after_asr_final(&mut session, "你好")`
    // assert phase == Thinking
}
```

Prefer testing a pure helper in `dialogue` that app/voice_sherpa call, so GPUI is not required.

- [ ] **Step 2: Run — FAIL**

- [ ] **Step 3: Implement sherpa VAD + streaming ASR adapters**

- Reuse model dir resolution from `voice_sherpa::model`
- Prefer silero/sherpa VAD config already documented upstream; pin paths under configurable model dir
- Keep recognizer creation off the foreground thread
- Unit tests may use fakes; add `#[ignore]` test for real model if assets exist

- [ ] **Step 4: Wire app background pump**

PCM receiver → vad/asr → channel events → app updates `DialogueSession` + `cx.notify()`

- [ ] **Step 5: Verify**

```bash
cargo test -p dialogue -p voice_sherpa
cargo check -p app
```

- [ ] **Step 6: Commit** (when requested)

```bash
git commit -m "$(cat <<'EOF'
Wire Mic to VAD and streaming ASR into dialogue Listening phase.

EOF
)"
```

---

### Task 6: Whole-Utterance TTS + Barge-In Stop

**Files:**
- Create: `crates/voice_sherpa/src/tts.rs`
- Modify: `crates/app/src/app/voice/playback.rs` — support stop; optionally accept 16k or 24k
- Modify: `crates/app/src/app/dialogue/` — Speaking phase + interrupt on VAD `SpeechStart` while Speaking
- Test: dialogue interrupt tests + tts unit tests

**Behavior:**

- Stub reply path for this plan: after Thinking, app may call a **fixed template** or echo transcript through TTS to prove the pipe (`synthesize` → sink `write_chunk` → `Speaking` → on complete back to `Listening`)
- On `VadEvent::SpeechStart` while `Speaking`: `sink.stop()`, `on_playback_finished_or_interrupted`, return `Listening`
- Card tasks are **not** cancelled on barge-in (hook only; full tasks in plan 02)

- [ ] **Step 1: Failing tests**

- `decide`/session: barge-in from Speaking → Listening, `models_resident` unchanged  
- TTS adapter: empty text → error; non-empty with fake backend returns PCM chunk  
- App-level: interrupt stops sink (use `Noop` sink that records `stop` calls)

- [ ] **Step 2: Implement TTS adapter**

Reuse patterns from `crates/voice_sherpa/tests/tts_demo.rs` (Matcha), but behind `TtsClient`. Model path configurable; missing model → clear error when entering Listening or first synthesize (choose **fail on Listening start** if TTS required for Always Listening — align with SPEC strictness for Ollama; for this plan, failing first synthesize with bar error is acceptable if documented in test).

- [ ] **Step 3: Verify**

```bash
cargo test -p dialogue -p voice_sherpa
cargo check -p app
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

- [ ] **Step 4: Commit** (when requested)

```bash
git commit -m "$(cat <<'EOF'
Add sherpa TTS client and barge-in stop for dialogue Speaking.

EOF
)"
```

---

## Plan 01 Exit Criteria

- [ ] `realtime` crate and `sidecars/openai-realtime` are gone from the workspace build
- [ ] PCM types live in `crates/audio`; no speech path depends on sidecar types
- [ ] Always Listening starts without OpenAI key
- [ ] Mic → VAD → ASR can produce a final transcript into `Thinking` (fake or real)
- [ ] TTS can speak a stub utterance; user speech interrupts playback
- [ ] Closing Listening keeps `models_resident`; window-close API clears it
- [ ] `cargo test -p audio -p dialogue -p voice_sherpa` and `cargo check -p app` pass

## Handoff To Plan 02

Plan 02 adds `crates/llm` (Mock → Ollama), flattens `card_task` tools (`Create`/`Focus`/`SendText` with multi-round `node_id`), template report playback queue, settings for Ollama, and e2e rewrite.
