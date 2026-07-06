# CEF Browser Engine Switch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a feature-gated Chromium/CEF browser engine path for Norma Browser cards while keeping the existing Wry/WebKit engine as the default.

**Architecture:** Introduce a small Browser engine abstraction under `crates/app/src/app/browser/` so `BrowserStore` delegates native view lifecycle calls to backend-specific code. Wry remains the default backend. CEF is compiled only with the `chromium-browser` feature and is selected by runtime configuration, avoiding CEF binary requirements for normal builds.

**Tech Stack:** Rust 2021, GPUI 0.2.2, wry 0.55.1, optional cef 149.3.0+149.0.6, macOS native child views.

## Global Constraints

- Domain crates stay zero GPUI; all browser native integration remains in `crates/app`.
- Default `cargo check -p app` must not require CEF binaries.
- `chromium-browser` feature gates all CEF imports and helper code.
- Wry remains the default engine until CEF packaging is fully validated.
- Rust files stay under 500 lines and do not use `#[allow(...)]`.
- Tests follow the repository's full comment and assertion-message requirements.

---

### Task 1: Browser Engine Selection Contract

**Files:**
- Modify: `crates/app/src/app/browser.rs`
- Create: `crates/app/src/app/browser/engine.rs`
- Test: `crates/app/tests/unit/browser_view_tests.rs`

**Interfaces:**
- Produces: `BrowserEngineKind::{Wry, Cef}` and `BrowserEngineKind::from_env_value(value: Option<&str>) -> BrowserEngineKind`
- Consumes: Existing `BrowserStore` construction.

- [ ] **Step 1: Write failing tests**
  - Add tests proving default selection is Wry, `cef/chromium` selects CEF, and unknown values fall back to Wry.

- [ ] **Step 2: Run test to verify failure**
  - Run: `cargo test -p app test_browser_engine`
  - Expected: compile failure because `BrowserEngineKind` does not exist.

- [ ] **Step 3: Implement minimal engine kind**
  - Add `engine.rs` with `BrowserEngineKind`.
  - Re-export it from `browser.rs`.

- [ ] **Step 4: Verify**
  - Run: `cargo test -p app test_browser_engine`
  - Expected: all new engine selection tests pass.

### Task 2: Backend Dispatch Boundary

**Files:**
- Modify: `crates/app/src/app/browser.rs`
- Modify: `crates/app/src/app/browser/native.rs`
- Create: `crates/app/src/app/browser/wry_backend.rs`
- Test: `crates/app/tests/unit/browser_view_tests.rs`

**Interfaces:**
- Produces: `NativeBrowserView` enum with Wry default backend.
- Consumes: Existing `BrowserView::{sync,set_bounds,set_visible,load_url,go_back,go_forward,reload}` lifecycle.

- [ ] **Step 1: Write failing source contract tests**
  - Assert `BrowserView` stores `NativeBrowserView` instead of direct `wry::WebView`.
  - Assert Wry-specific code lives in `wry_backend.rs`.

- [ ] **Step 2: Run test to verify failure**
  - Run: `cargo test -p app test_browser_backend`
  - Expected: failure because direct Wry storage still exists.

- [ ] **Step 3: Move Wry implementation behind backend enum**
  - Move Wry create/clip/rect code from `native.rs` to `wry_backend.rs`.
  - Keep geometry in `native.rs`.
  - Make `BrowserView` call `NativeBrowserView`.

- [ ] **Step 4: Verify**
  - Run: `cargo test -p app test_browser_backend`
  - Expected: pass.

### Task 3: CEF Feature Gate Scaffold

**Files:**
- Modify: `crates/app/Cargo.toml`
- Create: `crates/app/src/app/browser/cef_backend.rs`
- Modify: `crates/app/src/app/browser/engine.rs`
- Test: `crates/app/tests/unit/browser_view_tests.rs`

**Interfaces:**
- Produces: `#[cfg(all(target_os = "macos", feature = "chromium-browser"))]` CEF backend module boundary.
- Consumes: `BrowserEngineKind::Cef`.

- [ ] **Step 1: Write failing feature-gate tests**
  - Assert `chromium-browser` feature exists.
  - Assert CEF imports are isolated to `cef_backend.rs`.
  - Assert normal backend code does not import `cef`.

- [ ] **Step 2: Run test to verify failure**
  - Run: `cargo test -p app test_browser_cef`
  - Expected: failure because feature and module do not exist.

- [ ] **Step 3: Add optional dependency and CEF module scaffold**
  - Add `chromium-browser = ["dep:cef"]`.
  - Add macOS optional `cef = { version = "149.3.0+149.0.6", optional = true, default-features = false }`.
  - Add `cef_backend.rs` with a `CefBrowserView` type that returns a clear unsupported error until helper packaging is wired.

- [ ] **Step 4: Verify default build**
  - Run: `cargo check -p app`
  - Expected: pass without CEF binary requirements.

### Task 4: CEF Runtime and Packaging Hook Design

**Files:**
- Create: `crates/app/src/bin/norma_cef_helper.rs`
- Modify: `crates/app/src/main.rs`
- Modify: `Packager.toml`
- Test: `scripts/test-packager-config.sh`

**Interfaces:**
- Produces: helper binary entry point guarded by `chromium-browser`.
- Consumes: CEF `execute_process` and `initialize` APIs.

- [ ] **Step 1: Write failing packaging tests**
  - Extend packager script test to require `norma-cef-helper` only when Chromium packaging is enabled.

- [ ] **Step 2: Run test to verify failure**
  - Run: `bash scripts/test-packager-config.sh`
  - Expected: failure before helper configuration exists.

- [ ] **Step 3: Add helper binary and packaging placeholders**
  - Add `norma-cef-helper` bin behind feature-aware build logic.
  - Add packager notes/scripts for CEF framework resources.

- [ ] **Step 4: Verify**
  - Run: `bash scripts/test-packager-config.sh`
  - Expected: pass.

### Task 5: Final Verification

**Files:**
- All files touched above.

- [ ] **Step 1: Run app tests**
  - Run: `cargo test -p app`
  - Expected: pass.

- [ ] **Step 2: Run app check**
  - Run: `cargo check -p app`
  - Expected: pass.

- [ ] **Step 3: Run Chromium feature check**
  - Run: `cargo check -p app --features chromium-browser`
  - Expected: pass if CEF binaries are available; otherwise report the exact missing binary/resource requirement and keep default build green.

- [ ] **Step 4: Run guardrails**
  - Run: `bash scripts/check-max-file-lines.sh`
  - Run: `bash scripts/check-no-allow.sh`
  - Expected: both pass.
