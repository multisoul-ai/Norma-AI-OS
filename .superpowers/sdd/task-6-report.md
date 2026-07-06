# Task 6 Report: App Realtime Voice Chat Plumbing With Fake Sidecar

## Commits

- Commit SHA: `3f79d225d40c8082b4142739ec3b0fc0fd5f5063`
- Commit message: `feat: wire realtime voice chat shell`
- Follow-up SHA: `28d3551`
- Follow-up message: `test: make app chrome filter runnable`

## RED Evidence

- `cargo test -p realtime test_assistant_audio_state_decodes_speaking_state -- --nocapture`
  - Failed before implementation with `no variant named AssistantAudioState found for enum RealtimeEvent`.
- `cargo test -p app app_realtime_tests_chat -- --nocapture`
  - Failed before implementation because `super::chat` and `crate::app::voice::playback` did not exist.

## GREEN Evidence

- Added protocol decode support for `assistant_audio_state`.
- Added app fake sidecar / fake PCM / fake assistant sink tests covering:
  - start stores PCM stream, starts sidecar, sends `start_session`, reaches Listening
  - PCM forwarding sends `input_audio_chunk`
  - assistant speaking maps label to `Replying...`
  - assistant audio chunks write to `AssistantAudioSink`
  - user PCM during assistant speaking sends `interrupt` and stops sink playback
  - stop cancels PCM, sends `stop_session`, clears session, restores `Voice`

## Changed Files

- `crates/app/src/app/realtime.rs`
- `crates/app/src/app/realtime/chat.rs`
- `crates/app/src/app/realtime/shell.rs`
- `crates/app/src/app/voice/mod.rs`
- `crates/app/src/app/voice/playback.rs`
- `crates/app/tests/unit/app_realtime_chat_tests.rs`
- `crates/app/tests/unit/app_realtime_chat_lifecycle_tests.rs`
- `crates/app/tests/unit/app_realtime_chat_support.rs`
- `crates/app/tests/unit/app_realtime_sidecar_tests.rs`
- `crates/realtime/src/protocol.rs`
- `crates/realtime/tests/unit/protocol_tests.rs`
- `crates/realtime/tests/fixtures/fake_sidecar.mjs`
- `crates/app/src/app/chrome.rs`

## Verification Results

- `cargo test -p app app_realtime_tests -- --nocapture` -> pass, 18 tests.
- `cargo test -p app app_chrome_tests -- --nocapture` -> pass, 9 tests.
- `cargo check -p app` -> pass.
- `bash scripts/check-no-allow.sh` -> pass.
- `bash scripts/check-max-file-lines.sh` -> pass.
- `cargo fmt --check` -> pass.
- `git diff --check` -> pass.
- Supplemental: `cargo test -p realtime -- --nocapture` -> pass, 16 tests across unit/integration targets.

## Residual Risk

- Runtime assistant playback sink is intentionally no-op/tracing for this task. Automated tests prove sink write/stop orchestration with a fake sink, but real speaker output is not proven and remains for the next playback task.

## Fix Report

- commit SHA: `ddcd3ac`
- root cause: `start_always_listening_shell` let a background startup finish after `stop_shell()` or after the GPUI entity was gone, but `RealtimeChatRuntime` had no cleanup path before install and the shell accepted stale completions. That left `SidecarProcess::stop(...)` and PCM cancellation uncalled on the abandoned runtime.
- test added:
  - `test_realtime_chat_drop_uninstalled_runtime_stops_pcm_and_sidecar`
  - `test_realtime_chat_stop_and_restart_rejects_stale_startup_attempt`
- exact verification command results:
  - `cargo test -p app app_realtime_tests_chat_lifecycle -- --nocapture` -> pass, 4 passed / 0 failed
  - `cargo test -p app app_realtime_tests -- --nocapture` -> pass, 20 passed / 0 failed
  - `cargo check -p app` -> pass
  - `bash scripts/check-no-allow.sh` -> pass
  - `bash scripts/check-max-file-lines.sh` -> pass
  - `cargo fmt --check` -> pass
  - `git diff --check` -> pass
- residual risk:
  - The real sidecar stop still happens on the existing background thread helper, so shutdown completion remains asynchronous by design; the tests only prove the stop path is triggered exactly once.

## Fix Report 2

- commit SHA: `f95cbd6`
- root cause:
  - `RealtimeChatRuntime::dispose()` and `RealtimeControlState::stop_chat_runtime()` both called `pcm_stream.stop()` inline on foreground cleanup paths.
  - For `PcmStreamSession`, `stop()` maps to `cancel()`, which can join capture/forward threads, so stale startup disposal and `stop_shell()` could block GPUI foreground updates.
- test/source guard added:
  - Added `test_realtime_chat_cleanup_source_guard_uses_background_pcm_stop_helper` to assert `dispose` and `stop_chat_runtime` use `stop_pcm_stream_in_background` and do not contain direct `pcm_stream.stop()` calls.
  - Updated RTC-6 / RTC-7 to wait for async PCM stop and async sidecar stop before asserting exact stop counts.
- exact verification results:
  - `cargo test -p app app_realtime_tests_chat_lifecycle -- --nocapture` -> pass, 5 passed / 0 failed
  - `cargo test -p app app_realtime_tests -- --nocapture` -> pass, 21 passed / 0 failed
  - `cargo check -p app` -> pass
  - `bash scripts/check-no-allow.sh` -> pass
  - `bash scripts/check-max-file-lines.sh` -> pass
  - `cargo fmt --check` -> pass
  - `git diff --check` -> pass
- residual risk:
  - PCM cleanup is now detached onto a background thread, so these tests prove non-blocking scheduling and exact-once ownership transfer, but they do not measure shutdown latency of the underlying audio backend itself.
