# Settings E2E Click Test Report

Date: 2026-07-07

## Goal

Cover the current Settings module with a sandboxed e2e test that clicks rendered UI elements and operates controls through the same GPUI mouse/key event path a user would exercise.

## Stack Decision

Selected stack: existing Rust `crates/e2e` + GPUI `test-support` / `TestAppContext` / `VisualTestContext::simulate_click`.

Rationale:

- This app is a native Rust/GPUI desktop app, not a web app, so Playwright is not a fit.
- GPUI exposes a test context for simulating platform input, which matches the current codebase and does not require a separately packaged app or macOS accessibility permission setup.
- Appium Mac2 and Apple XCTest are viable future L3/native-window options, but they depend on XCTest/accessibility automation. Appium documents Mac2 as an XCTest-based macOS automation driver, and its setup requires Xcode Helper accessibility/automation permissions. That is heavier than the project’s current headless e2e layer and needs stable accessibility identifiers.

Sources:

- GPUI docs.rs: `TestAppContext` supports simulated platform input: <https://docs.rs/gpui>
- Appium Mac2 driver docs: Mac2 automates macOS apps via Apple XCTest: <https://appium.github.io/appium.io/docs/en/drivers/mac2/>
- Appium Mac2 GitHub setup notes: accessibility/automation permissions are required: <https://github.com/appium/appium-mac2-driver>
- Apple UI automation guidance emphasizes preparing apps for automation with Accessibility: <https://developer.apple.com/videos/play/wwdc2025/344/>

## Implemented

- Added `crates/e2e/src/e2e_03_settings_clicks.rs`.
- The test opens Settings by clicking `toolbar-settings`, selects Keyboard Shortcuts by clicking `settings-section-keyboard`, clicks the New Terminal keycap, records `cmd-alt-n`, clicks Restore, then clicks Close.
- Added `debug_selector(...)` to rendered click targets so tests resolve real rendered bounds before calling `simulate_click`.
- Added `SettingsE2eSnapshot` in `crates/app/src/app/e2e.rs` for stable post-click assertions without exposing private settings structs.
- Fixed Settings layout so Keyboard rows and the content header shrink inside the fixed panel. The e2e test found that keycap and close bounds were previously rendered outside the panel, making true clicks impossible.

## Important GPUI Finding

`VisualTestContext::debug_bounds(selector)` is useful for locating rendered click targets, but it is not an absence oracle in GPUI 0.2.2: `Frame::clear` does not clear the debug bounds map between frames. Tests should use `debug_bounds` before clicking, then use app state snapshots or rendered replacement targets for negative assertions.

## Verification

Passed:

```bash
cargo test -p e2e
cargo test -p e2e -- --ignored
cargo check -p app
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

`cargo test -p e2e` result: 5 passed, 1 ignored. The ignored test is the existing real PTY smoke test.
`cargo test -p e2e -- --ignored` result: 1 passed.
