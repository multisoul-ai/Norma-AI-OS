# Realtime Card Task 01-03 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and automatically verify the first three MVP slices: Always Listening shell, Realtime sidecar IPC, and realtime voice-chat plumbing.

**Architecture:** Add a pure Rust `realtime` crate for Norma-owned session types, JSONL IPC, PCM chunk validation, and child-process sidecar management. Keep GPUI state in `crates/app`, keep microphone capture in `crates/voice_sherpa`, and keep the Node/TypeScript sidecar limited to OpenAI Realtime WebSocket adaptation. Runtime, canvas, terminal, and storage must remain free of OpenAI, WebSocket, sidecar, and GPUI dependencies.

**Tech Stack:** Rust 2021, GPUI `=0.2.2`, `crossbeam-channel`, `async-channel`, `serde`, `serde_json`, Node `v22+`, TypeScript sidecar, OpenAI Realtime WebSocket. Official docs verified on 2026-07-06: Realtime WebSocket examples use `wss://api.openai.com/v1/realtime?model=gpt-realtime-2`; Realtime function-call docs use `conversation.item.create`, `function_call_output`, `input_audio_buffer.append`, `session.update`, and `response.create`.

## Global Constraints

- Worktree path: `/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-01-03`.
- Specs covered by this plan: `docs/design-docs/realtime-card-task-mvp/01-always-listening-shell.md`, `02-realtime-sidecar-ipc.md`, and `03-realtime-voice-chat.md`.
- Existing one-shot voice command flow must remain available and continue to pass existing tests.
- `runtime/canvas/terminal/storage` must not depend on OpenAI, sidecar, HTTP, WebSocket, or GPUI new logic.
- `voice_sherpa` must not know OpenAI, sidecar, card task, or realtime tool calls.
- OpenAI API key must come from Settings for normal runtime; tests may inject fake config/providers explicitly.
- Do not log OpenAI API key, including sidecar command logs and error summaries.
- One app process supports at most one active realtime session.
- MVP default realtime model is `gpt-realtime-2`, but the model must remain configurable from Settings.
- `cargo build` baseline passed before this plan: exit 0, finished in 3m12s.
- `cargo test -p voice -p voice_sherpa -p app` baseline passed before this plan: exit 0.
- All new Rust tests must include the project-required Chinese doc comments with scenario, data construction, execution steps, and expected results.
- Every `assert!` and `assert_eq!` must include a failure message. Critical state checks require concrete positive and negative assertions.
- Do not use `#[allow(...)]`.
- Do not add synchronous SQLite writes or blocking IO to hot paths.
- For GPUI async work, store cancellable `Task` handles where lifecycle matters and call `cx.notify()` after state changes.

---

## File Structure

Create:

- `crates/realtime/Cargo.toml`: new pure Rust crate manifest.
- `crates/realtime/src/lib.rs`: public exports.
- `crates/realtime/src/audio.rs`: PCM format and chunk validation.
- `crates/realtime/src/protocol.rs`: Norma JSONL command/event structs and encode/decode.
- `crates/realtime/src/session.rs`: session ids, state machine, and user-facing status.
- `crates/realtime/src/sidecar.rs`: child-process bridge and API-key-safe logging.
- `crates/realtime/tests/unit/audio_tests.rs`: PCM validation tests.
- `crates/realtime/tests/unit/protocol_tests.rs`: JSONL encode/decode tests.
- `crates/realtime/tests/unit/session_tests.rs`: lifecycle and status tests.
- `crates/realtime/tests/unit/sidecar_tests.rs`: fake sidecar process tests.
- `crates/realtime/tests/fixtures/fake_sidecar.mjs`: deterministic JSONL sidecar for Rust tests.
- `sidecars/openai-realtime/package.json`: sidecar package scripts.
- `sidecars/openai-realtime/tsconfig.json`: TypeScript compiler config.
- `sidecars/openai-realtime/src/main.ts`: sidecar entrypoint.
- `sidecars/openai-realtime/test/ipc.test.mjs`: Node test for sidecar JSONL behavior.
- `crates/app/src/app/realtime.rs`: app-layer realtime lifecycle glue.
- `crates/app/tests/unit/app_realtime_tests.rs`: app realtime lifecycle unit tests.

Modify:

- `Cargo.toml`: add `crates/realtime` to workspace and workspace dependency.
- `crates/app/Cargo.toml`: depend on `realtime`.
- `crates/app/src/app/mod.rs`: add `realtime` module and `RealtimeControlState` field.
- `crates/app/src/app/lifecycle.rs`: stop realtime session on close/project transition if needed.
- `crates/app/src/app/voice/control.rs`: add realtime-aware visual/label composition without breaking one-shot phases.
- `crates/app/src/app/voice/bar.rs`: route bottom bar click behavior through app helper so realtime active mode does not start one-shot recording.
- `crates/app/src/app/voice/mod.rs`: share microphone input selection helper with realtime.
- `crates/app/src/settings/voice/state.rs`: add Settings-only realtime fields and validation.
- `crates/app/src/settings/voice/view.rs`: render `Realtime conversation` settings group.
- `crates/app/tests/unit/app_chrome_tests.rs`: update bottom bar expectations if needed.
- `crates/app/tests/unit/app_voice_tests.rs`: add helper tests if input-device helper visibility changes.
- `crates/app/tests/unit/settings_panel_tests.rs`: assert voice panel contains realtime anchors.
- `crates/app/tests/unit/voice_settings_tests.rs`: add realtime settings tests.
- `crates/voice_sherpa/src/lib.rs`: export PCM stream API.
- `crates/voice_sherpa/src/audio.rs`: support configurable target sample rate and PCM16 conversion.
- `crates/voice_sherpa/src/session.rs`: add stream-only session path that does not create a sherpa recognizer.
- `crates/voice_sherpa/tests/unit/audio_tests.rs`: add PCM conversion tests.
- `crates/voice_sherpa/tests/unit/session_state_tests.rs`: add stream session lifecycle tests if state is shared.

Do not modify:

- `crates/runtime/**`
- `crates/canvas/**`
- `crates/terminal/**`
- `crates/storage/**`, except no changes are expected for this plan.

---

### Task 1: Realtime Crate Core Types And JSONL Protocol

**Files:**
- Create: `crates/realtime/Cargo.toml`
- Create: `crates/realtime/src/lib.rs`
- Create: `crates/realtime/src/audio.rs`
- Create: `crates/realtime/src/protocol.rs`
- Create: `crates/realtime/src/session.rs`
- Create: `crates/realtime/tests/unit/audio_tests.rs`
- Create: `crates/realtime/tests/unit/protocol_tests.rs`
- Create: `crates/realtime/tests/unit/session_tests.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `PcmFormat`, `PcmAudioChunk`, `PcmAudioError`
- Produces: `RealtimeSessionId`, `RealtimeSessionConfig`, `RealtimeSessionState`, `RealtimeStatus`
- Produces: `RealtimeCommand`, `RealtimeEvent`, `RealtimeToolCall`, `RealtimeToolResult`
- Produces: `encode_command(command: &RealtimeCommand) -> Result<String, RealtimeProtocolError>`
- Produces: `decode_event(line: &str) -> Result<RealtimeEvent, RealtimeProtocolError>`
- Consumed later by: app realtime lifecycle and sidecar bridge.

- [ ] **Step 1: Write failing PCM validation tests**

Add tests covering:

```rust
PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0, 1, 2, 3])
PcmAudioChunk::new(PcmFormat::Pcm16, 0, 1, vec![0, 1])
PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 0, vec![0, 1])
PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0])
```

Expected assertions:

- Valid chunk returns `Ok`.
- Zero sample rate returns `InvalidSampleRate`.
- Zero channels returns `InvalidChannels`.
- Odd PCM16 byte length returns `InvalidPcm16Length`.

Run:

```bash
cargo test -p realtime --test unit -- audio
```

Expected before implementation: package or symbols missing.

- [ ] **Step 2: Implement `audio.rs` minimally**

Implement only the types and validation required by Step 1. `PcmAudioChunk` must derive `Clone`, `Debug`, `Eq`, `PartialEq`, `Serialize`, and `Deserialize`.

- [ ] **Step 3: Write failing JSONL protocol tests**

Add tests covering:

- `StartSession` encodes to one JSON object with `type:"start_session"`, `session_id`, `model`, and `tools`.
- `InputAudioChunk` encodes base64 data without adding a newline.
- `session_started` decodes to `RealtimeEvent::SessionStarted`.
- `assistant_audio_chunk` decodes into `PcmAudioChunk`.
- Unknown event type returns `RealtimeEvent::IgnoredUnknown { event_type }`, not panic.
- Malformed JSON returns `RealtimeProtocolError::MalformedJson`.

Run:

```bash
cargo test -p realtime --test unit -- protocol
```

Expected before implementation: symbols missing.

- [ ] **Step 4: Implement `protocol.rs` minimally**

Implement serde-tagged wire structs internally and expose stable domain enums. Use `base64` only in this crate if needed; add `base64 = "0.22"` to `crates/realtime/Cargo.toml`, not workspace-wide unless multiple crates require it.

- [ ] **Step 5: Write failing session-state tests**

Add tests covering:

- Initial state is `Off`.
- `start_connecting(session_id)` moves to `Connecting`.
- `mark_started()` moves to `Listening`.
- `mark_user_speaking(true)` moves display status to `UserSpeaking`.
- `mark_assistant_speaking(true)` moves display status to `AssistantSpeaking`.
- `mark_error("x")` moves to `Error`.
- Calling `start_connecting` while active returns an error preserving the existing session id.

Run:

```bash
cargo test -p realtime --test unit -- session
```

Expected before implementation: symbols missing.

- [ ] **Step 6: Implement `session.rs` minimally**

Keep this crate GPUI-free. State transitions should be pure functions/methods.

- [ ] **Step 7: Run task verification**

Run:

```bash
cargo test -p realtime
cargo tree -p realtime | rg -i "gpui|runtime|canvas|terminal|storage"; test $? -eq 1
bash scripts/check-no-allow.sh
```

Expected after implementation: tests pass; dependency grep has no output; no `#[allow]`.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/realtime
git commit -m "feat: add realtime protocol crate"
```

---

### Task 2: Always Listening Settings And Bottom Bar Shell

**Files:**
- Modify: `crates/app/Cargo.toml`
- Modify: `crates/app/src/app/mod.rs`
- Create: `crates/app/src/app/realtime.rs`
- Modify: `crates/app/src/app/voice/control.rs`
- Modify: `crates/app/src/app/voice/bar.rs`
- Modify: `crates/app/src/settings/voice/state.rs`
- Modify: `crates/app/src/settings/voice/view.rs`
- Modify: `crates/app/tests/unit/voice_settings_tests.rs`
- Create: `crates/app/tests/unit/app_realtime_tests.rs`
- Modify: `crates/app/tests/unit/settings_panel_tests.rs`

**Interfaces:**
- Consumes: Task 1 `realtime::{RealtimeSessionConfig, RealtimeSessionState, RealtimeStatus}`
- Produces: `VoiceSettingsValues::realtime_enabled() -> bool`
- Produces: `VoiceSettingsValues::openai_api_key() -> Option<&str>`
- Produces: `VoiceSettingsValues::realtime_model() -> Option<&str>`
- Produces: `VoiceSettingsValues::card_planner_model() -> Option<&str>`
- Produces: `VoiceSettingsValues::sidecar_path() -> Option<&Path>`
- Produces: `VoiceSettingsState::toggle_realtime_enabled()`
- Produces: `VoiceSettingsState::set_openai_api_key(String)`, `set_realtime_model(String)`, `set_card_planner_model(String)`, `set_sidecar_path(PathBuf)`
- Produces: `RealtimeControlState` in app layer with config validation and UI status mapping.

- [ ] **Step 1: Write failing settings tests**

Add tests covering:

- Realtime fields have no env fallback.
- Saved API key, realtime model, planner model, sidecar path, and enabled flag resolve into `VoiceSettingsValues`.
- Missing API key or model makes `RealtimeControlState::validate_start_config` return a config error.
- Default realtime model is `gpt-realtime-2` when saved model is missing.
- Realtime API key is never included in `Debug` output for values or validation errors.

Run:

```bash
cargo test -p app voice_settings_tests -- --nocapture
```

Expected before implementation: symbols missing.

- [ ] **Step 2: Implement settings state**

Add storage keys:

```rust
voice.realtime.enabled
voice.openai.api_key
voice.openai.realtime_model
voice.openai.card_planner_model
voice.openai.sidecar_path
```

Keep existing one-shot voice env fallback behavior unchanged for Ollama/Paraformer fields. Do not add env fallback for realtime OpenAI fields.

- [ ] **Step 3: Write failing settings UI tests**

Update `settings_panel_tests` to assert the Voice page contains these anchors:

```text
Realtime conversation
Enable Always Listening
OpenAI API Key
Realtime Model
Card Planner Model
Sidecar Path
```

Run:

```bash
cargo test -p app settings_panel_tests -- --nocapture
```

Expected before view implementation: missing anchors.

- [ ] **Step 4: Implement settings UI**

Add a `settings_group(theme, "Realtime conversation")` below one-shot command planning and above Advanced. Use existing `setting_row`, `value_pill`, and toggle visual style. The API key display must be masked as `••••••••` when present, never rendered raw.

- [ ] **Step 5: Write failing bottom-bar shell tests**

Add tests covering:

- `RealtimeControlState::default()` maps to `Voice`.
- Enabling valid realtime config maps bottom label to `Connecting...` then `Always Listening`.
- Config error maps bottom label to `Realtime error`.
- When realtime is active, bottom-bar mouse down does not start one-shot recording.
- Repeating start while active does not create a second session.

Run:

```bash
cargo test -p app app_realtime_tests -- --nocapture
```

Expected before implementation: symbols missing.

- [ ] **Step 6: Implement app realtime shell**

Add `RealtimeControlState` to `MultisoulApp`, but keep it independent from `VoiceControlState` internals. Add helper methods:

```rust
MultisoulApp::realtime_voice_label(&self) -> Option<String>
MultisoulApp::effective_voice_label(&self) -> String
MultisoulApp::effective_voice_visual(&self) -> VoiceVisual
MultisoulApp::start_always_listening_shell(&mut self, cx: &mut Context<Self>)
MultisoulApp::stop_always_listening_shell(&mut self, cx: &mut Context<Self>)
```

`render_voice_bar` should use effective label/visual, preserving one-shot behavior when realtime is off.

- [ ] **Step 7: Run task verification**

Run:

```bash
cargo test -p app voice_settings_tests -- --nocapture
cargo test -p app settings_panel_tests -- --nocapture
cargo test -p app app_realtime_tests -- --nocapture
cargo check -p app
bash scripts/check-no-allow.sh
```

Expected after implementation: all commands exit 0.

- [ ] **Step 8: Commit**

```bash
git add crates/app/Cargo.toml crates/app/src/app crates/app/src/settings crates/app/tests/unit
git commit -m "feat: add always listening shell"
```

---

### Task 3: Rust Sidecar Process Bridge And Fake Sidecar Acceptance

**Files:**
- Create: `crates/realtime/src/sidecar.rs`
- Modify: `crates/realtime/src/lib.rs`
- Create: `crates/realtime/tests/unit/sidecar_tests.rs`
- Create: `crates/realtime/tests/fixtures/fake_sidecar.mjs`
- Modify: `crates/app/src/app/realtime.rs`
- Modify: `crates/app/tests/unit/app_realtime_tests.rs`

**Interfaces:**
- Consumes: Task 1 command/event protocol.
- Produces: `SidecarLaunchConfig { executable: PathBuf, args: Vec<String>, openai_api_key: SecretString }`
- Produces: `SidecarProcess::spawn(config) -> Result<SidecarProcess, RealtimeSidecarError>`
- Produces: `SidecarProcess::send(&mut self, command: &RealtimeCommand) -> Result<(), RealtimeSidecarError>`
- Produces: `SidecarProcess::try_recv(&self) -> Option<Result<RealtimeEvent, RealtimeSidecarError>>`
- Produces: `SidecarProcess::stop(self, timeout: Duration) -> Result<(), RealtimeSidecarError>`

- [ ] **Step 1: Write failing sidecar bridge tests**

Tests must use `node crates/realtime/tests/fixtures/fake_sidecar.mjs`.

Cover:

- `start_session` sent by Rust causes fake sidecar to emit `session_started`.
- Fake sidecar emits malformed JSON when requested; Rust converts it to `RealtimeSidecarError::MalformedEvent`.
- Unknown event type is ignored and does not stop the process.
- `stop_session` causes fake sidecar to exit.
- Formatted launch errors and logs do not include the API key string.

Run:

```bash
cargo test -p realtime --test unit -- sidecar
```

Expected before implementation: symbols missing.

- [ ] **Step 2: Implement fake sidecar fixture**

The fixture should:

- Read newline-delimited JSON from stdin.
- On `start_session`, write `{"type":"session_started","session_id":"..."}`.
- On `stop_session`, write `{"type":"session_stopped","session_id":"...","reason":"user stopped"}` and exit.
- If config model equals `emit-malformed-json`, write a malformed line.
- If config model equals `emit-unknown-event`, write `{"type":"mystery","session_id":"..."}` then a valid `session_started`.

- [ ] **Step 3: Implement `sidecar.rs`**

Use `std::process::Command` with piped stdin/stdout/stderr. Read stdout on a background thread into `crossbeam_channel::Receiver`. Store child process and stdin writer in `SidecarProcess`. Put the OpenAI API key in the child environment variable `OPENAI_API_KEY`; never include it in command args.

- [ ] **Step 4: Wire app shell to sidecar bridge with fake-path support**

When Always Listening starts with a sidecar path, app should spawn sidecar and send `start_session`. On `session_started`, status becomes `Listening`. On start failure or session error, status becomes `Realtime error`.

Use fake sidecar in tests. Do not connect to OpenAI in this task.

- [ ] **Step 5: Run task verification**

Run:

```bash
cargo test -p realtime --test unit -- sidecar
cargo test -p app app_realtime_tests -- --nocapture
cargo check -p app
bash scripts/check-no-allow.sh
```

Expected after implementation: all commands exit 0.

- [ ] **Step 6: Commit**

```bash
git add crates/realtime crates/app/src/app/realtime.rs crates/app/tests/unit/app_realtime_tests.rs
git commit -m "feat: bridge realtime sidecar process"
```

---

### Task 4: Node/TypeScript Sidecar Minimum IPC Package

**Files:**
- Create: `sidecars/openai-realtime/package.json`
- Create: `sidecars/openai-realtime/tsconfig.json`
- Create: `sidecars/openai-realtime/src/main.ts`
- Create: `sidecars/openai-realtime/test/ipc.test.mjs`

**Interfaces:**
- Consumes: JSONL protocol from Task 1.
- Produces: `npm test` sidecar IPC tests.
- Produces: `npm run build` compiled JS at `sidecars/openai-realtime/dist/main.js`.

- [ ] **Step 1: Write failing Node IPC tests**

Use Node built-in `node:test`. Tests should spawn the built sidecar with:

```bash
node sidecars/openai-realtime/dist/main.js
```

Cover:

- `start_session` with `offline_echo: true` returns `session_started`.
- `stop_session` returns `session_stopped` then exits.
- malformed input returns `session_error`.
- no output contains the provided `OPENAI_API_KEY`.

Run:

```bash
cd sidecars/openai-realtime
npm install
npm run build
npm test
```

Expected before implementation: package or dist missing.

- [ ] **Step 2: Implement TypeScript sidecar offline IPC mode**

`src/main.ts` should support two modes:

- Offline IPC mode for automated tests when `start_session.config.offline_echo === true`.
- OpenAI mode for Task 7.

For Task 4, implement offline mode only and return a clear `session_error` if OpenAI mode is requested before Task 7.

- [ ] **Step 3: Run task verification**

Run:

```bash
cd sidecars/openai-realtime
npm install
npm run build
npm test
```

Expected after implementation: all commands exit 0.

- [ ] **Step 4: Commit**

```bash
git add sidecars/openai-realtime
git commit -m "feat: add realtime sidecar ipc package"
```

---

### Task 5: Voice Sherpa PCM Streaming API

**Files:**
- Modify: `crates/voice_sherpa/src/lib.rs`
- Modify: `crates/voice_sherpa/src/audio.rs`
- Modify: `crates/voice_sherpa/src/session.rs`
- Modify: `crates/voice_sherpa/tests/unit/audio_tests.rs`
- Modify: `crates/voice_sherpa/tests/unit/session_state_tests.rs` if shared lifecycle helpers are changed.
- Modify: `crates/voice_sherpa/Cargo.toml` only if a new dependency is unavoidable.

**Interfaces:**
- Consumes: `realtime::PcmAudioChunk` and `realtime::PcmFormat`
- Produces: `PcmStreamSession`
- Produces: `SherpaProvider::start_pcm_stream(input_device_id: Option<&str>) -> Result<(PcmStreamSession, async_channel::Receiver<PcmAudioChunk>), SpeechError>`
- Produces: `PcmStreamSession::cancel(self)`

- [ ] **Step 1: Write failing PCM conversion tests**

Cover:

- f32 samples `[-1.0, 0.0, 1.0]` convert to little-endian PCM16 bytes with saturation.
- Generated chunks use `sample_rate = 24_000`, `channels = 1`, `format = Pcm16`.
- Empty sample input does not send a chunk.
- Stream session type is `Send` where required by app background orchestration.

Run:

```bash
cargo test -p voice_sherpa audio_tests -- --nocapture
```

Expected before implementation: symbols missing.

- [ ] **Step 2: Implement stream-only audio path**

Reuse AVAudioEngine capture mechanics but do not create sherpa recognizer. Add configurable target sample rate to the internal audio path so one-shot recognition remains at 16 kHz and realtime stream emits 24 kHz PCM16 chunks.

- [ ] **Step 3: Write failing lifecycle tests**

Cover:

- Dropping `PcmStreamSession` stops audio capture.
- Calling `cancel()` twice is harmless through ownership or explicit state.
- Starting stream still requires microphone authorization.

Run:

```bash
cargo test -p voice_sherpa session_state_tests -- --nocapture
```

Expected before implementation: symbols missing or test failure.

- [ ] **Step 4: Run task verification**

Run:

```bash
cargo test -p voice_sherpa
cargo test -p app app_voice_tests -- --nocapture
bash scripts/check-no-allow.sh
```

Expected after implementation: commands exit 0; hardware-only ignored tests may remain ignored.

- [ ] **Step 5: Commit**

```bash
git add crates/voice_sherpa crates/app/tests/unit/app_voice_tests.rs
git commit -m "feat: add pcm stream capture"
```

---

### Task 6: App Realtime Voice Chat Plumbing With Fake Sidecar

**Files:**
- Modify: `crates/app/src/app/realtime.rs`
- Modify: `crates/app/src/app/mod.rs`
- Modify: `crates/app/src/app/voice/control.rs`
- Modify: `crates/app/src/app/voice/bar.rs`
- Create: `crates/app/src/app/voice/playback.rs`
- Modify: `crates/app/src/app/voice/mod.rs`
- Modify: `crates/app/tests/unit/app_realtime_tests.rs`
- Modify: `crates/app/tests/unit/app_chrome_tests.rs` if visual labels change.

**Interfaces:**
- Consumes: `voice_sherpa::SherpaProvider::start_pcm_stream`
- Consumes: `realtime::SidecarProcess`
- Produces: app fake sidecar acceptance path for input audio chunks, assistant audio chunks, speaking state, and interrupt.
- Produces: `AssistantAudioSink` abstraction with a test fake and a minimal runtime sink that records chunks if real playback is not available in automated tests.

- [ ] **Step 1: Write failing app realtime chat tests**

Tests use fake sidecar and fake PCM/audio sinks. Cover:

- Starting Always Listening starts PCM stream, starts sidecar, sends `start_session`, and reaches `Listening`.
- Receiving `assistant_audio_state:speaking` maps bottom label to `Replying...`.
- Receiving `assistant_audio_chunk` writes to `AssistantAudioSink`.
- User speaking while assistant is speaking sends `interrupt` and stops sink playback.
- Stopping Always Listening stops PCM stream, sends `stop_session`, clears active session, and returns bottom label to `Voice`.

Run:

```bash
cargo test -p app app_realtime_tests -- --nocapture
```

Expected before implementation: failures.

- [ ] **Step 2: Implement app orchestration**

Use GPUI `cx.spawn` and `background_executor` so permission requests, sidecar polling, and stream forwarding do not block foreground rendering. Store task handles in app realtime state so `stop_always_listening` cancels them by dropping handles.

- [ ] **Step 3: Implement assistant audio sink abstraction**

Automated tests only need a fake sink that records chunk counts and stop calls. Runtime path may use a minimal no-op sink with tracing if platform playback is not finished in this task; if no-op is used, the final report must list manual audio playback as a remaining risk for Task 7. Do not claim real speaker playback without a manual or automated proof.

- [ ] **Step 4: Run task verification**

Run:

```bash
cargo test -p app app_realtime_tests -- --nocapture
cargo test -p app app_chrome_tests -- --nocapture
cargo check -p app
bash scripts/check-no-allow.sh
```

Expected after implementation: all commands exit 0.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/app crates/app/tests/unit
git commit -m "feat: wire realtime voice chat shell"
```

---

### Task 7: OpenAI Realtime WebSocket Mode In Sidecar

**Files:**
- Modify: `sidecars/openai-realtime/src/main.ts`
- Modify: `sidecars/openai-realtime/test/ipc.test.mjs`
- Modify: `docs/design-docs/realtime-card-task-mvp/03-realtime-voice-chat.md` only if implementation clarifies an acceptance detail.

**Interfaces:**
- Consumes: Task 4 sidecar package.
- Produces: OpenAI mode using `WebSocket` global in Node 22.
- Produces: JSONL mappings:
  - Rust `start_session` -> OpenAI WebSocket connect + `session.update`
  - Rust `input_audio_chunk` -> `input_audio_buffer.append`
  - Rust `interrupt` -> response cancel/interrupt sequence supported by current docs
  - OpenAI assistant audio delta -> `assistant_audio_chunk`
  - OpenAI transcript/text delta -> `transcript_delta` / `assistant_text_delta`
  - OpenAI errors -> `session_error`

- [ ] **Step 1: Write failing sidecar OpenAI-mode mapping tests with fake WebSocket**

Do not call OpenAI in automated tests. Inject a fake WebSocket constructor from the test process and assert:

- Sidecar opens URL containing `wss://api.openai.com/v1/realtime?model=gpt-realtime-2` when model is `gpt-realtime-2`.
- It sends `session.update` with audio modalities and no API key in the payload.
- It maps Rust `input_audio_chunk` to `input_audio_buffer.append`.
- It maps fake OpenAI assistant audio delta to Norma `assistant_audio_chunk`.
- It maps fake OpenAI error to Norma `session_error`.

Run:

```bash
cd sidecars/openai-realtime
npm run build
npm test
```

Expected before implementation: failures.

- [ ] **Step 2: Implement OpenAI mode**

Use `OPENAI_API_KEY` from environment and send it only as an Authorization header. Do not include the API key in JSONL events, command-line args, or thrown error messages. Keep tool definitions for `enqueue_card_task` and `cancel_card_task` in `session.update`, but the Rust app will ignore tool calls until later specs.

- [ ] **Step 3: Run sidecar verification**

Run:

```bash
cd sidecars/openai-realtime
npm run build
npm test
```

Expected after implementation: all commands exit 0.

- [ ] **Step 4: Run integrated automatic acceptance**

Run:

```bash
cargo test -p realtime
cargo test -p voice_sherpa
cargo test -p app app_realtime_tests -- --nocapture
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

Expected after implementation: all commands exit 0.

- [ ] **Step 5: Commit**

```bash
git add sidecars/openai-realtime docs/design-docs/realtime-card-task-mvp/03-realtime-voice-chat.md
git commit -m "feat: connect realtime sidecar websocket"
```

---

## Final Automatic Acceptance

After all tasks are complete, run:

```bash
cargo build
cargo test -p realtime
cargo test -p voice_sherpa
cargo test -p app
cargo check -p app
cd sidecars/openai-realtime && npm run build && npm test
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

Record exact command outputs in the final implementation report. If real OpenAI credentials are not configured, do not run live Realtime manual acceptance; instead report that automatic fake-provider acceptance passed and live voice/audio acceptance remains manual.

## Plan Self-Review Checklist

- Spec 01 coverage: Task 2 covers Settings, bottom bar shell, active-session uniqueness, config validation, and close/stop shell behavior.
- Spec 02 coverage: Tasks 1, 3, and 4 cover JSONL protocol, Rust sidecar bridge, fake sidecar, malformed JSON, unknown events, and API-key-safe logging.
- Spec 03 coverage: Tasks 5, 6, and 7 cover PCM capture API, audio chunk transport, assistant audio events, interrupt, fake automatic acceptance, and OpenAI WebSocket mode.
- Scope exclusions preserved: no card task queue, no `enqueue_card_task` handling beyond sidecar tool definition, no runtime/canvas/storage dependency changes.
- Known risk: automated tests use fake sidecar and fake audio sink; live microphone/speaker/OpenAI acceptance requires credentials and hardware after implementation.
