# Final Fix Report

- Commit SHA: `35e97fd49bc61e92b706e8ec8d92d4e5afd936a4`

## Root cause per finding

### Critical: foreground realtime loop wrote to sidecar stdin directly

- `crates/app/src/app/realtime/shell.rs` called `this.update(... send_input_audio_chunk(...))`.
- `RealtimeControlState::send_input_audio_chunk` synchronously called sidecar command send, which reached blocking child-stdin `write_all + flush`.
- `RealtimeControlState::stop_shell` also synchronously sent `stop_session` before handing process shutdown to the background helper.
- Fix: introduced a background sidecar command worker in [`crates/app/src/app/realtime/chat.rs`](/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-01-03/crates/app/src/app/realtime/chat.rs), changed foreground paths to enqueue commands only, and changed `stop_shell` to `enqueue_stop_session` before background shutdown.

### Important 1: realtime audio validation accepted unsupported formats

- [`crates/realtime/src/audio.rs`](/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-01-03/crates/realtime/src/audio.rs) only rejected zero sample rate / zero channels, so 44.1kHz, 48kHz, and stereo PCM were incorrectly accepted.
- Fix: `PcmAudioChunk::new` now accepts only `24_000 Hz`, `1 channel`, `pcm16`.

### Important 2: missing sidecar path entered fake active shell state

- `validate_start_config` required API key / realtime model / planner model but not `sidecar_path`.
- `start_always_listening_shell` had a no-sidecar branch that marked the shell started, creating a false Always Listening session.
- Fix: added `RealtimeConfigError::MissingSidecarPath`, made startup validation require sidecar path, and removed the fake-start branch.

### Minor: Settings copy was outdated

- Voice settings UI still described sidecar path as a future bridge.
- Fix: updated copy to describe the real local sidecar executable or `.mjs` entry path.

## Tests added or updated

- Added realtime audio validation coverage for `44_100 Hz`, `48_000 Hz`, and `2 channels` rejection in [`crates/realtime/tests/unit/audio_tests.rs`](/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-01-03/crates/realtime/tests/unit/audio_tests.rs).
- Added sidecar-path validation coverage in [`crates/app/tests/unit/voice_settings_realtime_tests.rs`](/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-01-03/crates/app/tests/unit/voice_settings_realtime_tests.rs).
- Added config-error / process-slot coverage for missing sidecar path in [`crates/app/tests/unit/app_realtime_tests.rs`](/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-01-03/crates/app/tests/unit/app_realtime_tests.rs).
- Added source guard for enqueue-only foreground PCM loop and non-inline `stop_shell` send path in [`crates/app/tests/unit/app_realtime_chat_lifecycle_tests.rs`](/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-01-03/crates/app/tests/unit/app_realtime_chat_lifecycle_tests.rs).
- Updated sidecar source guard in [`crates/app/tests/unit/app_realtime_sidecar_tests.rs`](/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-01-03/crates/app/tests/unit/app_realtime_sidecar_tests.rs) to enforce detached cleanup on enqueue failure.
- Updated realtime chat test support in [`crates/app/tests/unit/app_realtime_chat_support.rs`](/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-01-03/crates/app/tests/unit/app_realtime_chat_support.rs) for the enqueue-based sidecar trait.

## Verification results

- `cargo test -p realtime` -> pass
- `cargo test -p app app_realtime_tests -- --nocapture` -> pass
- `cargo test -p app app_chrome_tests -- --nocapture` -> pass
- `cargo test -p app` -> pass
- `cargo check -p app` -> pass
- `cd sidecars/openai-realtime && npm run build` -> pass
- `cd sidecars/openai-realtime && npm test` -> pass
- `bash scripts/check-storage-boundaries.sh` -> pass
- `bash scripts/check-performance-guardrails.sh` -> pass
- `bash scripts/check-max-file-lines.sh` -> pass
- `bash scripts/check-no-allow.sh` -> pass
- `cargo fmt --check` -> pass
- `git diff --check` -> pass

## Residual risk

- The new sidecar command worker removes blocking child-stdin writes from GPUI foreground paths, but it is still a channel-backed async boundary. The tests prove enqueue-only foreground behavior and detached shutdown, not end-to-end UI latency under real microphone load.
- Runtime command-send failures from the worker surface asynchronously through sidecar event draining instead of synchronously from the foreground caller. Current tests cover the error mapping and detached cleanup path.
