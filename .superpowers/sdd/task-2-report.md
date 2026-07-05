# Task 2 Report: Always Listening Settings And Bottom Bar Shell

## 实现了什么

- Added app-layer `RealtimeControlState` shell in `crates/app/src/app/realtime.rs`.
- Added OpenAI realtime settings fields/getters/setters and storage keys:
  - `voice.realtime.enabled`
  - `voice.openai.api_key`
  - `voice.openai.realtime_model`
  - `voice.openai.card_planner_model`
  - `voice.openai.sidecar_path`
- Kept existing one-shot voice env fallback behavior unchanged.
- Added no env fallback for OpenAI realtime fields.
- Default realtime model is `gpt-realtime-2`.
- Masked OpenAI API key in Settings UI and custom `Debug` output as `••••••••`.
- Added Settings UI group `Realtime conversation`.
- Updated bottom voice bar to use realtime effective label/visual when realtime shell is active.
- Preserved one-shot bottom-bar behavior when realtime is disabled.
- Kept Task 2 shell-only: no sidecar process, real WebSocket, real mic stream, or playback.

## TDD Evidence

RED settings command:

```bash
cargo test -p app voice_settings_tests -- --nocapture
```

Key expected failures before implementation:

- `could not find realtime in app`
- `VoiceSettingsValues has no field named realtime_enabled/openai_api_key/realtime_model/...`
- missing getters for realtime settings fields

RED settings UI focused command:

```bash
cargo test -p app test_voice_settings_panel_contains_voice_configuration_anchors -- --nocapture
```

Key expected failure before UI implementation:

- `Voice 设置页应包含配置锚点: Realtime conversation`

RED bottom-bar command:

```bash
cargo test -p app app_realtime_tests -- --nocapture
```

Key expected failures before shell implementation:

- missing `VoiceBarMouseDownAction`
- missing `realtime_voice_label`
- missing `effective_voice_label`
- missing `effective_voice_visual`
- missing `start_shell_with_config`
- missing `mark_started`
- missing `mark_config_error`
- missing `current_session_id`

GREEN evidence:

- `cargo test -p app app_realtime_tests -- --nocapture` ran 5 tests, all passed.
- Earlier focused runs also passed:
  - `cargo test -p app test_realtime -- --nocapture`: 10 passed.
  - `cargo test -p app test_voice_settings_panel_contains_voice_configuration_anchors -- --nocapture`: 1 passed.

## 最终验证命令与结果

The brief commands were rerun during final closure:

```bash
cargo test -p app voice_settings_tests -- --nocapture
```

Exit 0. Current repo filter matched 0 tests because the mounted module is named `tests`, not `voice_settings_tests`.

```bash
cargo test -p app settings_panel_tests -- --nocapture
```

Exit 0. Current repo filter matched 0 tests because the mounted module is named `tests`, not `settings_panel_tests`.

```bash
cargo test -p app app_realtime_tests -- --nocapture
```

Exit 0. Current repo filter matched 0 tests after the app realtime tests were moved under `app::realtime::tests` to keep `app/mod.rs` below the 500-line cap.

The behavior-bearing focused tests were also rerun:

```bash
cargo test -p app test_realtime -- --nocapture
```

Exit 0. `10 passed; 0 failed`.

```bash
cargo test -p app test_voice_settings_panel_contains_voice_configuration_anchors -- --nocapture
```

Exit 0. `1 passed; 0 failed`.

```bash
cargo check -p app
```

Exit 0.

```bash
bash scripts/check-no-allow.sh
```

Exit 0.

```bash
cargo fmt --check
bash scripts/check-max-file-lines.sh
git diff --check
```

All exited 0 after the controller split oversized files.

## Controller line-count fix

The first commit attempt was blocked by the repo pre-commit hook:

```text
ERROR: Rust source file(s) exceed 500 lines.

714 crates/app/src/settings/voice/state.rs
591 crates/app/src/settings/voice/view.rs
573 crates/app/tests/unit/voice_settings_tests.rs
507 crates/app/src/app/mod.rs
505 crates/app/tests/unit/settings_panel_tests.rs
```

The controller fixed the root cause by splitting new realtime responsibilities:

- `crates/app/src/settings/voice/state/values.rs`: extracted settings values, storage keys, and API-key-safe Debug logic.
- `crates/app/src/settings/voice/view/realtime.rs`: extracted realtime settings group and toggle handler.
- `crates/app/tests/unit/voice_settings_realtime_tests.rs`: split realtime settings tests out of one-shot voice settings tests.
- `crates/app/tests/unit/settings_panel_voice_tests.rs`: split Voice settings anchor test out of the general settings panel tests.
- `crates/app/src/app/realtime.rs`: now owns `app_realtime_tests` mounting.
- `crates/app/src/app/wallpaper_resolvers.rs`: now owns `wallpaper_resolvers_tests` mounting so `app/mod.rs` stays below the cap.

`bash scripts/check-max-file-lines.sh` then passed.

## 修改文件列表

- `.superpowers/sdd/task-2-report.md`
- `Cargo.lock`
- `crates/app/Cargo.toml`
- `crates/app/src/app/mod.rs`
- `crates/app/src/app/realtime.rs`
- `crates/app/src/app/voice/bar.rs`
- `crates/app/src/app/voice/mod.rs`
- `crates/app/src/app/wallpaper_resolvers.rs`
- `crates/app/src/settings/panel.rs`
- `crates/app/src/settings/voice/state.rs`
- `crates/app/src/settings/voice/state/values.rs`
- `crates/app/src/settings/voice/view.rs`
- `crates/app/src/settings/voice/view/realtime.rs`
- `crates/app/tests/unit/app_realtime_tests.rs`
- `crates/app/tests/unit/settings_panel_tests.rs`
- `crates/app/tests/unit/settings_panel_voice_tests.rs`
- `crates/app/tests/unit/voice_settings_tests.rs`
- `crates/app/tests/unit/voice_settings_realtime_tests.rs`
- `crates/app/tests/unit/wallpaper_resolvers_tests.rs`

## 自审发现与风险

- The original brief filters still match 0 tests due existing module names; focused behavior-bearing tests were rerun and passed.
- `Cargo.lock` changed because `app` now depends on `realtime`; `voice/mod.rs` contains the bottom-bar mouse handlers needed to route realtime vs one-shot behavior.
- API key is stored in memory/settings value for startup validation, but not shown raw in UI, custom `Debug`, validation error `Debug`, or validation error `Display`.

## Review Fix

Task review found four missing behaviors:

- Enabling `Enable Always Listening` only saved the flag and did not start/validate the realtime shell.
- `validate_start_config` did not require `Card Planner Model`.
- An existing OpenAI API key could not be cleared from Settings by submitting an empty API-key edit.
- Project open / app drop did not stop the realtime shell state.

Fixes applied:

- `on_voice_settings_toggle_realtime` now calls `start_always_listening_shell(cx)` when enabling, and still calls `stop_always_listening_shell(cx)` when disabling.
- `RealtimeConfigError::MissingCardPlannerModel` was added; `validate_start_config` now requires non-empty API key, realtime model, and card planner model.
- Empty realtime-field edits now clear saved realtime fields; this specifically makes API key clearable and leaves the UI row at `Not set`.
- `open_workspace_path` and `Drop for MultisoulApp` now call `self.realtime.stop_shell()`.
- Added regression tests for planner-model validation, API-key clearing, and start/stop wiring.

RED evidence:

```bash
cargo test -p app test_realtime -- --nocapture
```

Before the fix, 3 tests failed:

- `test_realtime_toggle_and_lifecycle_are_wired_to_start_and_stop_paths`
- `test_realtime_openai_api_key_empty_commit_clears_saved_key`
- `test_realtime_validate_start_config_requires_card_planner_model`

GREEN evidence:

```bash
cargo test -p app test_realtime -- --nocapture
```

Exit 0. `13 passed; 0 failed`.

```bash
cargo test -p app test_voice_settings -- --nocapture
cargo test -p app test_voice_settings_panel_contains_voice_configuration_anchors -- --nocapture
cargo check -p app
cargo fmt --check
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
git diff --check
```

All exited 0.

## Third Review Fix

Final review found the brief acceptance filters matched zero or missed split test modules after
large test files were split to satisfy line-count limits.

Fixes applied:

- Renamed the mounted realtime test module to `app_realtime_tests`.
- Renamed the mounted voice settings modules to `voice_settings_tests` and
  `voice_settings_tests_realtime` so the brief filter covers both original and realtime settings
  tests.
- Renamed the mounted settings panel modules to `settings_panel_tests` and
  `settings_panel_tests_voice` so the brief filter covers both original and voice anchors.

GREEN evidence:

```bash
cargo test -p app voice_settings_tests -- --nocapture
```

Exit 0. `12 passed; 0 failed`.

```bash
cargo test -p app settings_panel_tests -- --nocapture
```

Exit 0. `10 passed; 0 failed`.

```bash
cargo test -p app app_realtime_tests -- --nocapture
```

Exit 0. `7 passed; 0 failed`.

```bash
cargo fmt --check
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
git diff --check
```

All exited 0.

## Second Review Fix

Second review found that the per-window `RealtimeControlState` enforced only one active
session inside a single app/window state, but not across multiple windows in the same
process.

Fixes applied:

- Added a process-level `ACTIVE_REALTIME_SESSION: Mutex<Option<RealtimeSessionId>>` guard in
  `crates/app/src/app/realtime.rs`.
- `start_shell_with_config` now acquires the process slot before entering `Connecting`.
- `stop_shell` releases the process slot for the owning session.
- `mark_config_error` releases any existing slot before moving to error state.
- Added `test_realtime_process_guard_blocks_second_independent_state_until_first_stops` to prove
  two independent `RealtimeControlState` instances cannot both become active, and the second can
  start only after the first stops.
- Serialized realtime state tests that acquire the process guard so Rust's parallel test runner
  cannot create false failures.

RED evidence:

```bash
cargo test -p app test_realtime_process_guard -- --nocapture
```

Before the fix, the new test failed to compile because `reset_process_guard_for_tests` did not
exist, proving there was no process-level guard API to reset or exercise.

GREEN evidence:

```bash
cargo test -p app test_realtime -- --nocapture
```

Exit 0. `14 passed; 0 failed`.

```bash
cargo test -p app test_voice_settings -- --nocapture
cargo test -p app test_voice_settings_panel_contains_voice_configuration_anchors -- --nocapture
cargo check -p app
cargo fmt --check
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
git diff --check
```

All exited 0.
