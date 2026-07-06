# Task 1 Report: Realtime Crate Core Types And JSONL Protocol

## 实现了什么

- Added the new pure-Rust `realtime` workspace crate.
- Implemented PCM core types:
  - `PcmFormat`
  - `PcmAudioChunk`
  - `PcmAudioError`
- Implemented realtime session core types:
  - `RealtimeSessionId`
  - `RealtimeSessionConfig`
  - `RealtimeSessionState`
  - `RealtimeStatus`
  - `RealtimeSessionError`
- Implemented JSONL protocol domain types and codecs:
  - `RealtimeCommand`
  - `RealtimeEvent`
  - `RealtimeToolCall`
  - `RealtimeToolResult`
  - `encode_command`
  - `decode_event`
  - `RealtimeProtocolError`
- Kept `crates/realtime` independent from GPUI, runtime, canvas, terminal, and storage.
- Kept JSONL encoding newline-free; writers remain responsible for line framing.
- Did not add any API key or OpenAI secret/config field to Task 1 types.
- Used the repository's existing mounted unit-test convention instead of `cargo test -p realtime --test unit -- ...`, because this repo mounts `crates/<crate>/tests/unit/*.rs` from source modules with `#[path = ...]` and has no `unit` integration-test target.

## TDD Evidence

### RED 1: PCM validation tests before audio implementation

Command:

```bash
cargo test -p realtime audio
```

Result:

- Exit code: `101`
- Key failure:

```text
unresolved imports `super::PcmAudioChunk`, `super::PcmAudioError`, `super::PcmFormat`
```

Why this was expected:

- The crate/test scaffold existed, but the PCM API required by Step 1 did not exist yet.
- This proved the audio test was exercising missing Task 1 behavior.

### GREEN 1: PCM validation after audio implementation

Command:

```bash
cargo test -p realtime audio
```

Result:

- Exit code: `0`
- `1 passed; 0 failed`

### RED 2: JSONL protocol tests before protocol implementation

Command:

```bash
cargo test -p realtime protocol
```

Result:

- Exit code: `101`
- Key failures:

```text
unresolved imports `crate::session::RealtimeSessionConfig`, `crate::session::RealtimeSessionId`
unresolved imports `super::decode_event`, `super::encode_command`, `super::RealtimeCommand`, `super::RealtimeEvent`, `super::RealtimeProtocolError`
```

Why this was expected:

- The protocol tests compiled against the intended public API, but the session id/config and protocol symbols were not implemented yet.

### GREEN 2: JSONL protocol after implementation

Command:

```bash
cargo test -p realtime protocol
```

Result:

- Exit code: `0`
- `6 passed; 0 failed; 1 filtered out`

### RED 3: session-state tests before state implementation

Command:

```bash
cargo test -p realtime session
```

Result:

- Exit code: `101`
- Key failure:

```text
unresolved imports `super::RealtimeSessionError`, `super::RealtimeSessionState`, `super::RealtimeStatus`
```

Why this was expected:

- The state tests exercised the intended pure session state machine before the state/status/error types existed.

### GREEN 3: session-state after implementation

Command:

```bash
cargo test -p realtime session
```

Result:

- Exit code: `0`
- `4 passed; 0 failed; 5 filtered out`

## 最终验证命令与结果

Commands run fresh before commit:

```bash
cargo test -p realtime
cargo tree -p realtime | rg -i "gpui|runtime|canvas|terminal|storage"; test $? -eq 1
bash scripts/check-no-allow.sh
```

Results:

- `cargo test -p realtime`: exit code `0`; `9 passed; 0 failed`; doc-tests `0 passed; 0 failed`.
- Dependency-boundary grep command: exit code `0`; no forbidden dependency output.
- `bash scripts/check-no-allow.sh`: exit code `0`.

## 修改文件列表

- `Cargo.toml`
- `Cargo.lock`
- `crates/realtime/Cargo.toml`
- `crates/realtime/src/lib.rs`
- `crates/realtime/src/audio.rs`
- `crates/realtime/src/protocol.rs`
- `crates/realtime/src/session.rs`
- `crates/realtime/tests/unit/audio_tests.rs`
- `crates/realtime/tests/unit/protocol_tests.rs`
- `crates/realtime/tests/unit/session_tests.rs`
- `.superpowers/sdd/task-1-report.md`

## 自审发现与风险

- `Cargo.lock` changed because Cargo added the new workspace package entry for `realtime`; it should be committed with the crate to keep locked builds stable.
- `start_session` encodes `model` and `tools` at the top level, matching the Task 1 brief. Some broader draft docs still mention nested `config`; downstream tasks should use the Task 1 contract or update intentionally.
- No GPUI/runtime/canvas/terminal/storage dependency was introduced.
- No `#[allow(...)]` was added.
- The crate remains pure state/protocol/audio logic; no sidecar process, app UI, storage, or API-key behavior was added in Task 1.
