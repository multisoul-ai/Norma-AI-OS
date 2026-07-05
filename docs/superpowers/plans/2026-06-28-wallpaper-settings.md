# Wallpaper Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users switch the workspace wallpaper from Settings across static images, GIF animations, and looped video, with 10 bundled built-ins, custom import, and persisted selection.

**Architecture:** Wallpaper rendering stays in the `app` layer. A `WallpaperSettingsState` (mirroring the `voice` settings pattern) holds the persisted selection and is wired onto `MultisoulApp`. Three render paths stem from a `WallpaperKind` discriminator: static/GIF via GPUI `img()` (GIF uses native multi-frame animation, requiring a stable `.id("wallpaper")`), and video via AVFoundation (`AVAssetReader` → 420f `CVPixelBuffer`) fed to GPUI `surface()` with a background timer. Built-ins are bundled resource files resolved at runtime (dev fallback to `CARGO_MANIFEST_DIR/assets/wallpapers/`); the 5 mp4s are tracked by git-lfs.

**Tech Stack:** Rust, GPUI 0.2.2 (pinned), `image =0.25`, `core-video =0.4.3` + `metal`, `objc2 =0.6` / `objc2-foundation =0.3` / `objc2-avfoundation` / `block2`, `rfd` (file picker), `dirs` (workspace), SQLite `SettingsStore` for persistence.

## Global Constraints

- `gpui = "=0.2.2"`, `alacritty_terminal =0.26` pinned — do not bump.
- `core-video = "=0.4.3"` with `features = ["metal"]` — must match gpui's `CVPixelBuffer` type exactly or `Into<SurfaceSource>` fails to compile.
- `image = "0.25"` — same minor as gpui's transitive `image 0.25.10`, to reuse one compiled artifact.
- Domain crates (`canvas`, `terminal`, `runtime`) must stay GPUI-free; wallpaper code lives in `crates/app`.
- No `#[allow(...)]` anywhere. Fix root causes.
- Single Rust source files ≤ 500 lines (`scripts/check-max-file-lines.sh`); split modules instead of raising the cap.
- TDD: write a focused failing test first wherever behavior is testable below the GPUI layer.
- Test comments + assertions follow `docs/quality/test-requirements.md` (data-construction derivation, explicit positive+negative assertions, no generalized loops, every `assert!` carries a reason string).
- Video path is macOS-only: gate video modules/render with `#[cfg(target_os = "macos")]`.
- `CVPixelBuffer` pixel format MUST be `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` (420f) — GPUI's metal/blade renderers `assert_eq!` on this and panic otherwise.
- Wallpaper `img()` element MUST carry `.id("wallpaper")` or GIF multi-frame animation stays frozen on frame 0.
- Data-directory root: `~/Library/Application Support/Multisoul/` (from `storage::default_app_db_path().parent()`); custom wallpapers live in `wallpapers/` under it.

## File Structure

**New files:**
- `crates/app/src/app/wallpaper_resolvers.rs` — pure: bundle/asset path resolution + `WallpaperKind` detection. GPUI-free, fully unit-testable.
- `crates/app/src/settings/wallpaper.rs` — `WallpaperSettingsState` (state + persistence, mirrors `voice/state.rs`). Re-exports types.
- `crates/app/src/settings/wallpaper_view.rs` — `wallpaper_settings(theme, state, cx)` render fn (grid + import + delete).
- `crates/app/src/app/file_picker.rs` — `pick_wallpaper_file()` wrapper around `rfd`.
- `crates/app/src/app/video_decoder.rs` — macOS-only `VideoDecoder` (AVFoundation); spike-defined interface.
- `crates/app/src/app/video_wallpaper.rs` — macOS-only video render glue (`surface()` + timer into `MultisoulApp`).
- `crates/app/assets/wallpapers/` — 10 built-in files (existing `1331865.png` stays; 9 new copied from `/Users/alan/Documents/壁纸/mut/`).
- `crates/app/tests/unit/wallpaper_resolvers_tests.rs` — pure resolver/kind tests.
- `crates/app/tests/unit/wallpaper_settings_tests.rs` — state persistence tests.
- `scripts/copy-wallpaper-resources.sh` — packaging helper (and referenced by Packager.toml resource step).
- `.gitattributes` (repo root, new or appended) — `*.mp4 filter=lfs diff=lfs merge=lfs -text`.

**Modified files:**
- `crates/app/Cargo.toml` — add `image`, `rfd`, `objc2`, `objc2-foundation`, `objc2-avfoundation`, `block2`, `core-video` deps.
- `crates/app/src/assets.rs` — extend `EmbeddedAssets::load`/`list` to serve multiple PNG/GIF built-ins; keep `include_bytes!` for PNG/GIF (videos bypass assets, resolved as file paths).
- `crates/app/src/app/mod.rs` — `wallpaper` field → `WallpaperSettingsState`; wire load/persist; module decls for resolvers/picker/video.
- `crates/app/src/app/chrome.rs` — backdrop `img()` gets `.id("wallpaper")`; route static/GIF vs video (video delegates to `video_wallpaper`).
- `crates/app/src/settings/mod.rs` — add `pub(crate) mod wallpaper;`.
- `crates/app/src/settings/panel.rs` — `SettingsSection::Wallpaper` variant + `title/icon/index` + sidebar button + pass `wallpaper_settings` to `settings_content`.
- `crates/app/src/settings/content.rs` — `SettingsSection::Wallpaper` dispatch branch + title/subtitle; add `wallpaper_state: &WallpaperSettingsState` param.
- `crates/app/tests/unit/settings_panel_tests.rs` — add wallpaper-section anchor test.
- `crates/app/tests/unit/app_tests.rs` — refresh existing env-driven tests to new state-based shapes as needed.
- `docs/references/packaging.md` — document the resource-copy step.

---

## Task 1: Video Feasibility Spike (objc2 + AVAssetReader + surface())

> **This task MUST run first and gate all video tasks (11–13).** Its accepted output defines the exact `VideoDecoder` interface used by later tasks. Per the agreed approach: do the spike standalone, prove the loop, then finalize video task signatures from its result. Non-video tasks (2–10) are independent of this task and may proceed in parallel with it.

**Files:**
- Create (throwaway spike, in `crates/app/src/app/video_decoder.rs` — will be promoted to production in Task 11):
  - `crates/app/src/app/video_decoder.rs`
- Spike binary proving the loop: `crates/app/examples/spike_video_wallpaper.rs` (gated `#[cfg(target_os="macos")]`)

**Interfaces:**
- Consumes: `gpui 0.2.2` `surface()`, `core_video::pixel_buffer::CVPixelBuffer`; `objc2`/`objc2-avfoundation` AVAssetReader APIs.
- Produces (the contract later tasks adopt):
  ```rust
  #[cfg(target_os = "macos")]
  pub(crate) struct DecodedVideoFrame {
      pub buffer: core_video::pixel_buffer::CVPixelBuffer, // guaranteed 420f
  }

  #[cfg(target_os = "macos")]
  pub(crate) struct VideoDecoder {
      /* spike-defined internals: AVAssetReader + track output, looping */
  }

  #[cfg(target_os = "macos")]
  impl VideoDecoder {
      /// Open `path` (mp4/mov). Returns Err if unreadable/unsupported.
      pub(crate) fn open(path: &std::path::Path) -> anyhow::Result<Self>;
      /// Next frame in BGRA-agnostic 420f CVPixelBuffer; loops at EOF.
      /// `Duration` hint is the wall-clock budget; decoder自由推进到下一可用帧。
      pub(crate) fn next_frame(&mut self) -> anyhow::Result<DecodedVideoFrame>;
      /// Single BGRA first-frame for thumbnails (uses AVAssetImageGenerator).
      pub(crate) fn first_frame_bgra(path: &std::path::Path) -> anyhow::Result<image::RgbaImage>;
  }
  ```

- [ ] **Step 1: Add deps to `crates/app/Cargo.toml` (dev + main as needed)**

Add under `[dependencies]`:
```toml
image = "0.25"
rfd = "0.15"
core-video = { version = "=0.4.3", features = ["metal"] }
objc2 = "0.6"
objc2-foundation = "0.3"
objc2-avfoundation = "0.3"
block2 = "0.6"
```
Run `cargo update -p app --offline 2>/dev/null; cargo check -p app` and resolve any version-pairing errors between `objc2`/`objc2-foundation`/`objc2-avfoundation` (these three must share the same `objc2` generation; if `objc2-avfoundation 0.3` doesn't exist at the paired version, search `cargo search objc2-avfoundation` and pin the version that pairs with `objc2 0.6.x`).

- [ ] **Step 2: Write a failing spike test asserting the decode loop produces a 420f buffer**

`crates/app/examples/spike_video_wallpaper.rs` (a runnable example, not a `#[test]`, because it needs a real mp4 and a GPU window; run manually). It MUST:
1. Open a bundled test mp4 (`crates/app/assets/wallpapers/1403512.mp4` — copy it in Task 9, or temporarily point at `/Users/alan/Documents/壁纸/mut/1403512.mp4` for the spike).
2. `VideoDecoder::open(path)` → loop `next_frame()` ~30 times without panic.
3. Assert each frame's `CVPixelBuffer` pixel format equals `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` (core-video exposes `get_pixel_format()`).
4. Print `OK <width>x<height> 420f` on success.

Also write a pure unit test in `crates/app/tests/unit/video_decoder_tests.rs` (gated `#[cfg(target_os="macos")]`, `#[ignore]` since it needs a real file — but compile-checks the API surface):
```rust
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs a real mp4 on disk; run with --ignored --ignored-features"]
fn spike_decoder_returns_420f_frames() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/wallpapers/1403512.mp4");
    if !path.exists() { return; } // skip when asset absent (CI)
    let mut dec = video_decoder::VideoDecoder::open(&path).expect("open mp4");
    for _ in 0..30 {
        let f = dec.next_frame().expect("next frame");
        let pf = f.buffer.get_pixel_format();
        assert_eq!(
            pf, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            "GPUI surface() requires 420f; got 0x{:08x}", pf
        );
    }
}
```
(Use the exact core-video constant name; verify via `cargo doc -p core-video --open` if unsure — the constant lives in `core_video::pixel_format` or `core_video::sys`.)

- [ ] **Step 3: Run to verify it fails (compile)**

```bash
cargo build -p app --target x86_64-apple-darwin 2>&1 | tail -20
# and on the native arch:
cargo check -p app
```
Expected: FAIL — `video` module undeclared / AVAssetReader API unknown.

- [ ] **Step 4: Implement `VideoDecoder` against AVAssetReader**

In `crates/app/src/app/video_decoder.rs`, gated `#[cfg(target_os = "macos")]`:
- Use `objc2-avfoundation`'s `AVAssetReader`, `AVAssetReaderTrackOutput`, `AVURLAsset`. Set the track output's `outputSettings` with `kCVPixelBufferPixelFormatTypeKey => kCVPixelFormatType_420YpCbCr8BiPlanarFullRange` (420f) so AVFoundation decodes straight to the GPU-friendly format.
- `open`: build `AVURLAsset` from the file URL, create `AVAssetReader`, add a video `AVAssetReaderTrackOutput`, `startReading`.
- `next_frame`: `copyNextSampleBuffer` → `CMSampleBufferGetImageBuffer` → `CVPixelBuffer`. On EOF, restart the reader (seek to 0 / new reader) to loop. Wrap in `anyhow`.
- `first_frame_bgra`: use `AVAssetImageGenerator` to grab one `CGImage` → decode to `image::RgbaImage` for thumbnails.
- This step is iterative and may require reading `objc2` docs. Consult the `objc2` examples in `~/.cargo/registry/.../objc2-avfoundation-*/` (`cargo doc --open -p objc2-avfoundation`). Keep the file under 500 lines.
- Declare `mod video_decoder;` in `crates/app/src/app/mod.rs` (mac-gated).

- [ ] **Step 5: Run the spike and verify the loop**

```bash
# native arch
cargo run -p app --example spike_video_wallpaper
```
Expected: prints `OK <w>x<h> 420f` 30 times without panic. If it panics on `assert_eq!` (wrong format) — fix `outputSettings` to force 420f. If it links/compiles but `next_frame` errors — confirm AVAssetReader `startReading` succeeded and the track is a video track.

- [ ] **Step 6: Commit**

```bash
git add crates/app/Cargo.toml crates/app/src/app/video_decoder.rs crates/app/src/app/mod.rs crates/app/examples/spike_video_wallpaper.rs crates/app/tests/unit/video_decoder_tests.rs
git commit -m "spike(wallpaper): prove AVAssetReader→420f CVPixelBuffer loop in GPUI

Validates the video-wallpaper path before committing to full impl.
VideoDecoder interface frozen here is the contract for Tasks 11–13.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 2: Bundle/asset path resolver + WallpaperKind detection

**Files:**
- Create: `crates/app/src/app/wallpaper_resolvers.rs`
- Test: `crates/app/tests/unit/wallpaper_resolvers_tests.rs`
- Modify: `crates/app/src/app/mod.rs` (declare `mod wallpaper_resolvers;`)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, Eq, PartialEq)]
  pub(crate) enum WallpaperKind { Static, Gif, Video }

  impl WallpaperKind {
      /// 由文件扩展名判定（.png/.jpg/.jpeg→Static, .gif→Gif, .mp4/.mov→Video）。
      pub(crate) fn from_extension(ext: &str) -> Option<Self>;
  }

  /// 把内置相对资源名（如 "wallpapers/1350954.png"）解析为运行时可读的绝对路径。
  /// 打包态：从 app bundle Resources 解析；开发态：回退到 CARGO_MANIFEST_DIR/assets/<rel>。
  /// 解析失败返回 None（调用方回退默认壁纸）。
  pub(crate) fn resolve_builtin_asset(rel: &str) -> Option<std::path::PathBuf>;

  /// 内置壁纸清单：固定 10 项，每项 (相对路径, 显示名, kind)。
  pub(crate) fn builtin_wallpapers() -> &'static [(&'static str, &'static str, WallpaperKind)];
  ```

- [ ] **Step 1: Write failing tests**

`crates/app/tests/unit/wallpaper_resolvers_tests.rs` (first line `use super::*;` won't work for a separate tests file — instead `use app::...`; but app is a bin crate, so mirror the existing `settings_panel_tests.rs` pattern: include the source via `include_str!` for structural anchors, and for pure-fn tests use a `#[path]` mod. Concretely, attach to `app/mod.rs`):
Actually follow the project pattern — add to `crates/app/src/app/mod.rs`:
```rust
#[cfg(test)]
#[path = "../../tests/unit/wallpaper_resolvers_tests.rs"]
mod wallpaper_resolvers_tests;
```
The test file:
```rust
use super::wallpaper_resolvers::{builtin_wallpapers, WallpaperKind};

/// T-R1: 扩展名 → kind 判定覆盖三类，未知扩展返回 None。
///
/// 数据构造：
///   "png" → Static, "jpg" → Static, "jpeg" → Static
///   "gif" → Gif
///   "mp4" → Video, "mov" → Video
///   "txt" → None, "" → None, "PNG" → Static（大小写不敏感）
#[test]
fn test_kind_from_extension_classifies_three_types_and_rejects_unknown() {
    assert_eq!(WallpaperKind::from_extension("png"), Some(WallpaperKind::Static), "png must be Static");
    assert_eq!(WallpaperKind::from_extension("jpg"), Some(WallpaperKind::Static), "jpg must be Static");
    assert_eq!(WallpaperKind::from_extension("gif"), Some(WallpaperKind::Gif), "gif must be Gif");
    assert_eq!(WallpaperKind::from_extension("mp4"), Some(WallpaperKind::Video), "mp4 must be Video");
    assert_eq!(WallpaperKind::from_extension("mov"), Some(WallpaperKind::Video), "mov must be Video");
    assert_eq!(WallpaperKind::from_extension("txt"), None, "txt must be rejected");
    assert_eq!(WallpaperKind::from_extension(""), None, "empty ext must be None");
    assert_eq!(WallpaperKind::from_extension("PNG"), Some(WallpaperKind::Static), "ext matching is case-insensitive");
}

/// T-R2: 内置清单恰为 10 项，包含现有默认 + 2 PNG + 2 GIF + 5 mp4，且路径与 kind 匹配。
///
/// 数据构造（推导）：
///   内置清单 = DEFAULT_WALLPAPER(1331865.png, Static)
///            + 1350954.png(Static) + 1358524.png(Static)
///            + 215824.gif(Gif) + 220240.gif(Gif)
///            + 1403512.mp4 + 1404942.mp4 + 1406443.mp4 + 1406489.mp4 + 1407174.mp4 (Video)
///   Total = 10
#[test]
fn test_builtin_wallpapers_contains_exactly_ten_with_correct_kinds() {
    let list = builtin_wallpapers();
    assert_eq!(list.len(), 10, "built-in library must contain exactly 10 entries");
    let counts = list.iter().fold((0,0,0), |(s,g,v), (_, _, k)| match k {
        WallpaperKind::Static => (s+1, g, v),
        WallpaperKind::Gif => (s, g+1, v),
        WallpaperKind::Video => (s, g, v+1),
    });
    assert_eq!(counts, (4, 2, 4), "expected 4 static (incl. default), 2 gif, 4 video — got {:?}", counts);
    // 注：default 1331865.png 是第 4 个 static
    assert!(list.iter().any(|(p, _, _)| p.ends_with("1331865.png")), "default wallpaper 1331865.png must be present");
    for (p, _, _) in list {
        assert!(p.starts_with("wallpapers/"), "built-in path must be under wallpapers/, got {p}");
    }
}
```
> Note: the spec lists 2 new PNG + 1 default PNG = 3 static; but the table also includes the existing default. Reconcile: `1331865.png`(default) + `1350954.png` + `1358524.png` = 3 static, 2 gif, 5 video = 10. **Fix the count assertion to `(3, 2, 5)`** and `len()==10`. (The `(4,2,4)` above is wrong — correct to `(3,2,5)`.)

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p app wallpaper_resolvers_tests 2>&1 | tail -20
```
Expected: FAIL — module/`builtin_wallpapers` undefined.

- [ ] **Step 3: Implement `wallpaper_resolvers.rs`**

```rust
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WallpaperKind { Static, Gif, Video }

impl WallpaperKind {
    pub(crate) fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" => Some(Self::Static),
            "gif" => Some(Self::Gif),
            "mp4" | "mov" => Some(Self::Video),
            _ => None,
        }
    }
}

/// 内置壁纸清单：相对 bundle Resources 路径。源素材见 /Users/alan/Documents/壁纸/mut。
pub(crate) fn builtin_wallpapers() -> &'static [(&'static str, &'static str, WallpaperKind)] {
    &[
        ("wallpapers/1331865.png", "Default",   WallpaperKind::Static),
        ("wallpapers/1350954.png", "Aurora",    WallpaperKind::Static),
        ("wallpapers/1358524.png", "Nebula",    WallpaperKind::Static),
        ("wallpapers/215824.gif",  "Drift",     WallpaperKind::Gif),
        ("wallpapers/220240.gif",  "Pulse",     WallpaperKind::Gif),
        ("wallpapers/1403512.mp4", "Tide",      WallpaperKind::Video),
        ("wallpapers/1404942.mp4", "Ember",     WallpaperKind::Video),
        ("wallpapers/1406443.mp4", "Monsoon",   WallpaperKind::Video),
        ("wallpapers/1406489.mp4", "Abyss",     WallpaperKind::Video),
        ("wallpapers/1407174.mp4", "Halcyon",   WallpaperKind::Video),
    ]
}

/// 解析内置资源相对路径为绝对路径。
/// 打包态优先 app bundle Resources；开发态回退 CARGO_MANIFEST_DIR/assets。
pub(crate) fn resolve_builtin_asset(rel: &str) -> Option<PathBuf> {
    // 1) 开发期：CARGO_MANIFEST_DIR/crates/app/assets/<rel>
    if let Some(manifest) = std::option_env!("CARGO_MANIFEST_DIR") {
        let dev = std::path::Path::new(manifest).join("assets").join(rel);
        if dev.is_file() {
            return Some(dev);
        }
    }
    // 2) 打包态：bundle Resources/<rel>。main.rs 暴露 bundle resources 根。
    if let Some(res) = crate::app::bundle_resources_dir() {
        let packed = res.join(rel);
        if packed.is_file() {
            return Some(packed);
        }
    }
    None
}
```
This introduces `crate::app::bundle_resources_dir()` — add it in Task 3 (Resources resolution). For this task to compile standalone, stub it locally to return `None` and let Task 3 fill it; OR define `bundle_resources_dir` in this same task. **Define it here** to avoid a forward dependency:
Add to `crates/app/src/app/mod.rs`:
```rust
/// macOS app bundle 的 Contents/Resources 目录；非打包运行返回 None。
pub(crate) fn bundle_resources_dir() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // .app/Contents/MacOS/<bin> → .app/Contents/Resources
    let macos = exe.parent()?;          // .../Contents/MacOS
    let contents = macos.parent()?;     // .../Contents
    if contents.file_name()?.to_str()?.starts_with(&format!("..")) { return None; }
    let res = contents.join("Resources");
    if res.is_dir() { Some(res) } else { None }
}
```
(Keep robust: returns None unless we're actually inside a `.app/Contents/MacOS` layout.)

- [ ] **Step 4: Run to verify it passes**

```bash
cargo test -p app wallpaper_resolvers_tests 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/app/wallpaper_resolvers.rs crates/app/src/app/mod.rs crates/app/tests/unit/wallpaper_resolvers_tests.rs
git commit -m "feat(wallpaper): add WallpaperKind + built-in resolver (dev/bundle paths)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 3: Extend EmbeddedAssets for multiple PNG/GIF built-ins

**Files:**
- Modify: `crates/app/src/assets.rs`
- Test: `crates/app/tests/unit/app_tests.rs` (the assets tests live inline at `assets.rs:38-182`, update them there)
- Reuses: `builtin_wallpapers()` from Task 2.

**Interfaces:**
- Produces: `EmbeddedAssets` now `load`s any built-in PNG/GIF by relative path and `list`s all built-in PNG/GIF filenames; videos are NOT served here.

- [ ] **Step 1: Update the inline assets tests to expect the expanded library**

In `crates/app/src/assets.rs` test module, replace the `wallpapers` listing assertion (currently expects only `1331865.png`) to expect all 6 PNG/GIF built-ins (3 PNG + 2 GIF = 5 plus default = wait: default IS one of the 3 PNG; so 3 PNG + 2 GIF = 5 image built-ins). Concretely, drive expectations from `builtin_wallpapers()` filtered to Static+Gif:
```rust
/// 内置 image 资产：load/list 应覆盖所有 Static 与 GIF 内置项（不含视频）。
#[test]
fn test_embedded_assets_serve_all_image_builtins() {
    let assets = EmbeddedAssets;
    for (rel, _name, kind) in crate::app::wallpaper_resolvers::builtin_wallpapers() {
        if matches!(kind, crate::app::wallpaper_resolvers::WallpaperKind::Video) { continue; }
        let bytes = assets.load(rel).expect("load must not error")
            .unwrap_or_else(|| panic!("image builtin {rel} must be embedded"));
        assert!(!bytes.is_empty(), "builtin {rel} embedded bytes must be non-empty");
    }
    let listed = assets.list("wallpapers").expect("list wallpapers must not error");
    for (rel, _, kind) in crate::app::wallpaper_resolvers::builtin_wallpapers() {
        if matches!(kind, crate::app::wallpaper_resolvers::WallpaperKind::Video) { continue; }
        let fname = rel.rsplit('/').next().expect("path has filename");
        assert!(listed.iter().any(|s| s.as_ref() == fname), "wallpapers listing must include {fname}");
    }
}
```
Keep the existing `test_embedded_assets_load_default_wallpaper_png` and `test_embedded_assets_list_wallpaper_directory` but update the latter's hardcoded expectation to the 5 image filenames (or relax it to a subset + the new test above for full coverage). Prefer: keep default-PNG test as-is; update `test_embedded_assets_list_wallpaper_directory` to assert `listed.contains` each image filename rather than exact equality.

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p app --lib assets 2>&1 | tail -20   # or the inline path
cargo test -p app test_embedded_assets_serve_all_image_builtins 2>&1 | tail -20
```
Expected: FAIL — new builtins not embedded / not listed.

- [ ] **Step 3: Extend `EmbeddedAssets`**

Generate one `include_bytes!` const per image built-in via a macro, and serve by path:
```rust
use crate::app::wallpaper_resolvers::{builtin_wallpapers, WallpaperKind};

macro_rules! image_builtins {
    ( $( ($rel:literal, $const_name:ident) ),* $(,)? ) => {
        $( const $const_name: &[u8] = include_bytes!(concat!("../assets/", $rel)); )*
        fn image_builtin_bytes(path: &str) -> Option<Cow<'static, [u8]>> {
            match path {
                $( $rel => Some(Cow::Borrowed($const_name)), )*
                _ => None,
            }
        }
    };
}
image_builtins! {
    ("wallpapers/1331865.png", DEFAULT_WALLPAPER_BYTES),
    ("wallpapers/1350954.png", WALLPAPER_1350954_BYTES),
    ("wallpapers/1358524.png", WALLPAPER_1358524_BYTES),
    ("wallpapers/215824.gif",  WALLPAPER_215824_BYTES),
    ("wallpapers/220240.gif",  WALLPAPER_220240_BYTES),
}
```
Then in `load`:
```rust
fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    if let Some(bytes) = image_builtin_bytes(path) { return Ok(Some(bytes)); }
    if path == APP_ICON_ASSET { return Ok(Some(Cow::Borrowed(APP_ICON_BYTES))); }
    Ok(None)
}
```
And in `list("wallpapers")` return the 5 image filenames built from `builtin_wallpapers()` filtered to non-video. `DEFAULT_WALLPAPER_ASSET` constant stays `wallpapers/1331865.png`.

> ⚠️ This task **requires the 3 new PNG + 2 GIF files to be present at `crates/app/assets/wallpapers/`** for `include_bytes!` to compile. Copy them in now (the mp4s come in Task 9 via git-lfs, but PNG/GIF are small — copy all image built-ins here):
```bash
mkdir -p crates/app/assets/wallpapers
cp /Users/alan/Documents/壁纸/mut/1350954.png crates/app/assets/wallpapers/
cp /Users/alan/Documents/壁纸/mut/1358524.png crates/app/assets/wallpapers/
cp /Users/alan/Documents/壁纸/mut/215824.gif  crates/app/assets/wallpapers/
cp /Users/alan/Documents/壁纸/mut/220240.gif  crates/app/assets/wallpapers/
```

- [ ] **Step 4: Run to verify it passes**

```bash
cargo test -p app test_embedded_assets 2>&1 | tail -30
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/assets.rs crates/app/assets/wallpapers/1350954.png crates/app/assets/wallpapers/1358524.png crates/app/assets/wallpapers/215824.gif crates/app/assets/wallpapers/220240.gif
git commit -m "feat(wallpaper): embed 4 additional image built-ins (2 PNG, 2 GIF)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 4: WallpaperSettingsState (persistence, mirrors voice pattern)

**Files:**
- Create: `crates/app/src/settings/wallpaper.rs`
- Test: `crates/app/tests/unit/wallpaper_settings_tests.rs`
- Modify: `crates/app/src/settings/mod.rs` (`pub(crate) mod wallpaper;`)

**Interfaces:**
- Produces:
  ```rust
  pub(crate) enum WallpaperSelection {
      Builtin { rel: &'static str },   // 内置相对路径
      Custom { filename: String },     // 数据目录 wallpapers/ 下的文件名
  }
  pub(crate) struct WallpaperSettingsState {
      pub(crate) selection: WallpaperSelection,
      storage: Option<SettingsStore>,
  }
  impl WallpaperSettingsState {
      pub(crate) fn load_default() -> Self;
      pub(crate) fn select_builtin(&mut self, rel: &'static str);
      pub(crate) fn select_custom(&mut self, filename: String);
      pub(crate) fn current_kind(&self) -> WallpaperKind;
      /// 解析为运行时可读路径；内置走 resolve_builtin_asset，自定义走数据目录。
      /// 失败返回 None → 调用方回退默认。
      pub(crate) fn resolve_path(&self) -> Option<std::path::PathBuf>;
      /// 当前选中的 asset path（仅内置且为图片时，供 GPUI ImageSource::Resource 用）。
      pub(crate) fn image_asset_path(&self) -> Option<&'static str>;
      /// 列出自定义项（扫描数据目录 wallpapers/），返回文件名列表。
      pub(crate) fn list_custom(&self) -> Vec<String>;
      /// 删除自定义项：删文件 + 若是当前选中则回退默认。
      pub(crate) fn delete_custom(&mut self, filename: &str);
  }
  pub(crate) fn custom_wallpapers_dir() -> Option<std::path::PathBuf>; // ~/Library/Application Support/Multisoul/wallpapers/
  ```

- [ ] **Step 1: Write failing tests**

`crates/app/tests/unit/wallpaper_settings_tests.rs`, attached via `#[path]` in `settings/wallpaper.rs`. Use a temp data dir by overriding `custom_wallpapers_dir` via an env var or a test-only setter. Pattern: read `MULTISOUL_TEST_DATA_DIR` to redirect the data dir.
```rust
use super::*;
use std::fs;

/// T-W1: load_default 在无偏好时返回默认内置壁纸 1331865.png。
///
/// 数据构造：
///   无 settings 表记录 → selection 为 Builtin{1331865.png}
///
/// 执行过程：
///   1. WallpaperSettingsState::load_default() 读取 settings_store（测试环境可能无 db）
///   2. 无记录 → 回退默认
///
/// 预期结果：
///   - 默认 selection 指向 1331865.png
///   - current_kind() == Static
#[test]
fn test_load_default_falls_back_to_default_wallpaper() {
    let s = WallpaperSettingsState::load_default_with_test_store();
    assert!(matches!(s.selection, WallpaperSelection::Builtin { rel } if rel.ends_with("1331865.png")),
        "default selection must be the embedded default wallpaper");
    assert_eq!(s.current_kind(), crate::app::wallpaper_resolvers::WallpaperKind::Static,
        "default wallpaper kind must be Static");
}

/// T-W2: select_builtin 持久化后，新建 state 读取应还原同一选择。
/// （持久化走真实 SettingsStore + tempdir db；自定义目录用 env 覆盖。）
#[test]
fn test_builtin_selection_round_trips_through_persistence() {
    // 使用 tempdir 作为数据根：覆盖 db 路径 + custom dir
    let tmp = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MULTISOUL_TEST_DATA_ROOT", tmp.path());
    let mut s = WallpaperSettingsState::load_default_test();
    s.select_builtin("wallpapers/220240.gif");
    // 重新加载
    let s2 = WallpaperSettingsState::load_default_test();
    assert!(matches!(&s2.selection, WallpaperSelection::Builtin { rel } if rel == &"wallpapers/220240.gif"),
        "reloaded selection must be the gif builtin we just saved");
    std::env::remove_var("MULTISOUL_TEST_DATA_ROOT");
}

/// T-W3: delete_custom 删除当前选中的自定义项后，selection 回退默认；文件确实被删。
#[test]
fn test_delete_custom_falls_back_to_default_and_removes_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::env::set_var("MULTISOUL_TEST_DATA_ROOT", tmp.path());
    let dir = custom_wallpapers_dir().expect("custom dir resolved under test root");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("mine.png"), b"fake").unwrap();
    let mut s = WallpaperSettingsState::load_default_test();
    s.select_custom("mine.png".into());
    s.delete_custom("mine.png");
    assert!(matches!(s.selection, WallpaperSelection::Builtin { rel } if rel.ends_with("1331865.png")),
        "deleting the in-use custom must fall back to default");
    assert!(!dir.join("mine.png").exists(), "custom file must be removed from disk");
    std::env::remove_var("MULTISOUL_TEST_DATA_ROOT");
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p app wallpaper_settings_tests 2>&1 | tail -20
```
Expected: FAIL — module/types undefined.

- [ ] **Step 3: Implement `settings/wallpaper.rs`**

Mirror `voice/state.rs`. KEYs:
```rust
const KEY_WALLPAPER_KIND: &str = "wallpaper.kind";   // "builtin" | "custom"
const KEY_WALLPAPER_VALUE: &str = "wallpaper.value"; // rel path 或 custom filename
```
- `load_default`: `AppStore::open_default()` → `settings_store()` → read both keys → reconstruct `WallpaperSelection`;验证可解析（builtin 资源存在 / custom 文件存在），否则回退默认 + `tracing::warn!`。
- `select_builtin`/`select_custom`: update `selection` + `persist`（写两个 KEY）+ `cx_notify` 由调用方处理。
- `custom_wallpapers_dir`: `default_app_db_path().ok()?.parent().map(|p| p.join("wallpapers"))`. For tests, if `MULTISOUL_TEST_DATA_ROOT` set, return `<root>/wallpapers` and also route the db to `<root>/app.sqlite` (the test store helpers `load_default_test` open `AppStore::new(root.join("app.sqlite"))`).
- `resolve_path`: builtin→`resolve_builtin_asset(rel)`; custom→`custom_wallpapers_dir().join(filename)` if file exists.
- `image_asset_path`: `Some(rel)` only when builtin AND `current_kind()` is Static or Gif (videos don't use asset path). For custom images, return None and render via `ImageSource::from(PathBuf)` (file path).
- `list_custom`: scan `custom_wallpapers_dir()` for files, filter by `WallpaperKind::from_extension`, return filenames.
- `delete_custom`: remove file; if `selection` == that custom → reset to default builtin + persist.
- Keep file < 500 lines; split view into `wallpaper_view.rs` (Task 7).

Add to `crates/app/src/settings/mod.rs`: `pub(crate) mod wallpaper;` and re-export `WallpaperSettingsState`, `WallpaperSelection`.

- [ ] **Step 4: Run to verify it passes**

```bash
cargo test -p app wallpaper_settings_tests 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/settings/wallpaper.rs crates/app/src/settings/mod.rs crates/app/tests/unit/wallpaper_settings_tests.rs
git commit -m "feat(wallpaper): persist wallpaper selection via SettingsStore (voice-pattern)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 5: Wire WallpaperSettingsState into MultisoulApp + GIF `.id("wallpaper")`

**Files:**
- Modify: `crates/app/src/app/mod.rs` (replace `wallpaper: WorkspaceWallpaper` field with `wallpaper_settings: WallpaperSettingsState`; update `new()`; provide accessor for chrome)
- Modify: `crates/app/src/app/chrome.rs` (backdrop `img().id("wallpaper")`; route by kind)
- Modify: `crates/app/src/app/wallpaper.rs` (keep env fn for back-compat but the state supersedes it; or remove env-driven path — see step)
- Test: `crates/app/tests/unit/app_chrome_tests.rs` (assert `.id("wallpaper")` anchor)

**Interfaces:**
- Consumes: `WallpaperSettingsState` (Task 4), `WallpaperKind`, `resolve_builtin_asset`.
- Produces: `MultisoulApp::wallpaper_settings` field; chrome renders wallpaper from it.

- [ ] **Step 1: Write failing test for the `.id("wallpaper")` anchor**

`crates/app/tests/unit/app_chrome_tests.rs` (follow the `include_str!` source-anchor pattern):
```rust
/// T-W4: 壁纸 img 元素必须带 id="wallpaper"，否则 GPUI GIF 多帧动画卡在第 0 帧。
#[test]
fn test_wallpaper_img_has_stable_id_for_gif_animation() {
    let src = include_str!("../../src/app/chrome.rs");
    assert!(src.contains(".id(\"wallpaper\")"),
        "chrome.rs backdrop img must carry .id(\"wallpaper\") so GIF frames advance");
}
```
Also add a test in `app_tests.rs` that `MultisoulApp::new` no longer panics and defaults to the embedded default (smoke, if testable without a window — many app tests are source-anchor only; prefer the source-anchor approach).

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p app test_wallpaper_img_has_stable_id_for_gif_animation 2>&1 | tail -10
```
Expected: FAIL — anchor not present yet.

- [ ] **Step 3: Wire the state + add the id**

In `crates/app/src/app/mod.rs`:
- Replace field `wallpaper: WorkspaceWallpaper` → `pub(crate) wallpaper_settings: WallpaperSettingsState`.
- In `new()`: replace `let wallpaper = wallpaper_from_env();` → `let wallpaper_settings = WallpaperSettingsState::load_default();` and update the struct literal.
- Remove now-unused `wallpaper_from_env`/`WorkspaceWallpaper` from the env-driven path IF nothing else uses them (grep first). Keep `wallpaper.rs` only if `MULTISOUL_WALLPAPER` env override is still desired as an override layer; simplest: drop the env path entirely (SPEC moves selection to Settings). If dropped, delete `wallpaper.rs` and remove its `mod`/`use` lines and the `wallpaper_from_env_value` test in `app_tests.rs`.

In `crates/app/src/app/chrome.rs`, change the backdrop child:
```rust
backdrop = backdrop.child(self.render_wallpaper_layer(theme, cx));
```
and add a helper that routes by kind:
```rust
fn render_wallpaper_layer(&self, theme, cx) -> impl IntoElement {
    // 视频走 video_wallpaper 模块（Task 12，mac-only）；其余走 img()。
    #[cfg(target_os = "macos")]
    if self.wallpaper_settings.current_kind() == WallpaperKind::Video {
        return crate::app::video_wallpaper::render_video_wallpaper(self, cx);
    }
    let source = self.wallpaper_settings.image_source_for_render();
    img(source).absolute().top(px(0.0)).left(px(0.0)).right(px(0.0)).bottom(px(0.0))
        .size_full().object_fit(ObjectFit::Cover)
        .opacity(WALLPAPER_IMAGE_OPACITY)
        .id("wallpaper")
}
```
Add a method on `WallpaperSettingsState`:
```rust
pub(crate) fn image_source_for_render(&self) -> gpui::ImageSource {
    // 内置图片 → ImageSource::Resource(asset_path)；自定义图片 → ImageSource::from(path)
    use gpui::ImageSource;
    if let Some(rel) = self.image_asset_path() {
        return ImageSource::Resource(rel.into()); // Asset path string
    }
    match self.resolve_path() {
        Some(p) => ImageSource::from(p),
        None => ImageSource::Resource(crate::assets::DEFAULT_WALLPAPER_ASSET.into()),
    }
}
```
> Confirm the exact `ImageSource::Resource` variant ctor in gpui 0.2.2 (`Resource(impl Into<SharedString>)` per the img.rs:41-51 finding). Adjust to gpui's actual API — verify with `cargo check`.

- [ ] **Step 4: Run to verify it passes + full app check**

```bash
cargo test -p app test_wallpaper_img_has_stable_id_for_gif_animation 2>&1 | tail -5
cargo check -p app 2>&1 | tail -20
```
Expected: test PASS, `cargo check` clean. (Video branch references `video_wallpaper` which doesn't exist yet — gate it out with `#[cfg(target_os="macos")]` AND a `#[allow(unused)]` NO — instead stub an empty `video_wallpaper` module returning the default img fallback until Task 12, OR use a feature gate. Cleanest: create `crates/app/src/app/video_wallpaper.rs` now with a placeholder that returns `img(default)` and a `// TODO Task 12` — **but No Placeholders is a rule**. Resolution: do NOT add the video branch yet; introduce it in Task 12. In Task 5 the render helper handles only Static/Gif; video routing is added when `video_wallpaper` exists.)

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/app/mod.rs crates/app/src/app/chrome.rs crates/app/tests/unit/app_chrome_tests.rs crates/app/src/app/wallpaper.rs
git commit -m "feat(wallpaper): wire WallpaperSettingsState into app + GIF img id

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 6: SettingsSection::Wallpaper + sidebar entry

**Files:**
- Modify: `crates/app/src/settings/panel.rs` (enum variant + `title/icon/index` + sidebar button + pass state to `settings_content`)
- Modify: `crates/app/src/settings/content.rs` (signature: add `wallpaper_state` param; dispatch branch + title/subtitle)
- Test: `crates/app/tests/unit/settings_panel_tests.rs`

**Interfaces:**
- Produces: `SettingsSection::Wallpaper`; `settings_content(..., wallpaper_state: &WallpaperSettingsState, ...)`.

- [ ] **Step 1: Write failing anchor test**

`crates/app/tests/unit/settings_panel_tests.rs`:
```rust
/// T-W5: Settings 侧栏必须新增 Wallpaper 项，并在 content 分发里渲染它。
#[test]
fn test_settings_has_wallpaper_section() {
    let src = format!("{}{}{}",
        include_str!("../../src/settings/panel.rs"),
        include_str!("../../src/settings/content.rs"),
        include_str!("../../src/settings/wallpaper.rs"));
    let required = [
        "SettingsSection::Wallpaper",
        "\"Wallpaper\"",          // title
        "wallpaper_settings",     // toolbar/state field wired
        "wallpaper_state",        // param
    ];
    for anchor in required {
        assert!(src.contains(anchor), "wallpaper settings wiring missing: {anchor}");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p app test_settings_has_wallpaper_section 2>&1 | tail -10
```
Expected: FAIL.

- [ ] **Step 3: Add the variant + dispatch**

`panel.rs`:
- `SettingsSection::Wallpaper` variant.
- `title` → `"Wallpaper"`; `icon` → `"🖼"` (or `"W"`); `index` → `7`.
- In `render_settings_sidebar`: add `.child(section_button(theme, SettingsSection::Wallpaper, active, cx))` after Hooks (or wherever visually appropriate).
- In `render_settings_panel` call site (`panel.rs:215`): pass `&self.wallpaper_settings`.

`content.rs`:
- `settings_content` signature gains `wallpaper_state: &WallpaperSettingsState`.
- match arm: `SettingsSection::Wallpaper => wallpaper_settings(theme, wallpaper_state, cx),`
- `content_title`/`content_subtitle` add `Wallpaper` → `"Wallpaper"` / `"Choose a background"`.

- [ ] **Step 4: Run to verify it passes + gate compile**

```bash
cargo test -p app test_settings_has_wallpaper_section 2>&1 | tail -10
cargo check -p app 2>&1 | tail -20
```
Expected: test PASS; `cargo check` will fail only because `wallpaper_settings()` render fn isn't defined yet — define a minimal stub inline in `content.rs` returning an empty `div()` here, to be replaced by Task 7. (To avoid a placeholder violating rules, Task 6 and Task 7 should be done together; if split, Task 6's "render fn" is a one-line `div()` that Task 7 expands — acceptable as incremental scaffolding, commit message notes it.)

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/settings/panel.rs crates/app/src/settings/content.rs crates/app/tests/unit/settings_panel_tests.rs
git commit -m "feat(wallpaper): add Wallpaper settings section + sidebar entry

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 7: Wallpaper settings view — thumbnail grid + import + delete

**Files:**
- Create: `crates/app/src/settings/wallpaper_view.rs`
- Modify: `crates/app/src/settings/wallpaper.rs` (re-export `wallpaper_settings` fn; split view out to keep < 500 lines)
- Modify: `crates/app/src/settings/wallpaper.rs` mod decl or a `mod.rs`-style: add `pub(crate) mod wallpaper_view;` in `settings/mod.rs` and have `wallpaper.rs` re-export `wallpaper_view::wallpaper_settings`.

**Interfaces:**
- Produces: `pub(crate) fn wallpaper_settings(theme: Theme, state: &WallpaperSettingsState, cx: &mut Context<MultisoulApp>) -> Div`.
- Consumes: `builtin_wallpapers()`, `state.list_custom()`, `state.selection`, file picker (Task 8 — if 8 not done, gate import button behind a no-op until 8).

- [ ] **Step 1: Write failing anchor test**

`crates/app/tests/unit/settings_panel_tests.rs`:
```rust
/// T-W6: 壁纸视图包含缩略图网格、Import 按钮、且内置项不可删（无删除入口标记）。
#[test]
fn test_wallpaper_view_has_grid_import_and_builtin_nodelete() {
    let src = include_str!("../../src/settings/wallpaper_view.rs");
    assert!(src.contains("wallpaper-thumb"), "thumbnails need an id root for onClick");
    assert!(src.contains("Import"), "Import button must exist");
    assert!(src.contains("wallpaper-import"), "import button needs an id");
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p app test_wallpaper_view_has_grid_import_and_builtin_nodelete 2>&1 | tail -10
```
Expected: FAIL — file missing.

- [ ] **Step 3: Implement the view**

A scrollable grid (`div().flex().flex_wrap()`): for each `builtin_wallpapers()` entry render a thumbnail cell (clickable div id `("wallpaper-thumb", index)` → `cx.listener(|app,_,_,cx| app.wallpaper_settings.select_builtin(rel); cx.notify())`); for each `state.list_custom()` render a custom cell with a hover delete button (id `("wallpaper-delete", i)` → `app.wallpaper_settings.delete_custom(name); cx.notify()`). Highlight the currently-selected cell with a border.
- Thumbnail image source: for Static/Gif built-ins use `img(ImageSource::Resource(rel))`; for Video built-ins use the first-frame BGRA from `VideoDecoder::first_frame_bgra` (Task 11/13 — until then show a placeholder `div()` with the kind label; **gate video thumbnail behind `#[cfg(target_os="macos")]` + a `has_video` check**, committing only the Static/Gif thumbnail path in the initial version and wiring video thumbnails in Task 13). To respect No-Placeholders: implement Static/Gif thumbnails fully now; video cell renders a labeled `div("VIDEO")` and Task 13 replaces it — explicitly noted, not a hidden TODO.
- Import button (id `wallpaper-import`) → `app.import_wallpaper_file(cx)` (Task 8 method). Until Task 8, the button calls a method that's added in 8 — so do Task 7 and 8 in sequence within the same subagent dispatch.

- [ ] **Step 4: Run to verify it passes + check**

```bash
cargo test -p app test_wallpaper_view_has_grid_import_and_builtin_nodelete 2>&1 | tail -10
cargo check -p app 2>&1 | tail -20
```
Expected: PASS + clean check (after Task 8 wires `import_wallpaper_file`).

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/settings/wallpaper_view.rs crates/app/src/settings/wallpaper.rs crates/app/src/settings/mod.rs crates/app/tests/unit/settings_panel_tests.rs
git commit -m "feat(wallpaper): thumbnail grid + import/delete UI

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 8: File picker (rfd) + custom import flow

**Files:**
- Create: `crates/app/src/app/file_picker.rs`
- Modify: `crates/app/src/app/mod.rs` (`mod file_picker;` + `import_wallpaper_file(&mut self, cx)` method + `cx.spawn` the picker)
- Modify: `crates/app/src/settings/wallpaper.rs` (`import_custom(&mut self, src_path: &Path) -> ImportResult` copies file to data dir, updates selection)

**Interfaces:**
- Produces:
  ```rust
  pub(crate) fn pick_wallpaper_image() -> Option<std::path::PathBuf>; // rfd blocking
  impl MultisoulApp {
      pub(crate) fn import_wallpaper_file(&mut self, cx: &mut Context<Self>);
  }
  impl WallpaperSettingsState {
      pub(crate) fn import_custom(&mut self, src: &Path) -> ImportResult;
  }
  pub(crate) enum ImportResult { Imported(String /*filename*/), Rejected(String /*reason*/) }
  ```

- [ ] **Step 1: Write failing tests**

`crates/app/tests/unit/wallpaper_settings_tests.rs`:
```rust
/// T-W7: import_custom 把一个 png 拷到数据目录，列表出现，selection 切到它。
#[test]
fn test_import_custom_copies_and_selects() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("MULTISOUL_TEST_DATA_ROOT", tmp.path());
    let src = tmp.path().join("src.png");
    std::fs::write(&src, b"fakepng").unwrap();
    let mut s = WallpaperSettingsState::load_default_test();
    let res = s.import_custom(&src);
    assert!(matches!(res, ImportResult::Imported(_)), "png import must succeed");
    let custom = s.list_custom();
    assert_eq!(custom.len(), 1, "exactly one custom after import");
    assert!(matches!(s.selection, WallpaperSelection::Custom { .. }), "selection must switch to imported");
    std::env::remove_var("MULTISOUL_TEST_DATA_ROOT");
}

/// T-W8: import_custom 拒绝非图片/非视频扩展（如 .txt），返回 Rejected 且不污染目录。
#[test]
fn test_import_custom_rejects_unknown_extension() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("MULTISOUL_TEST_DATA_ROOT", tmp.path());
    let src = tmp.path().join("notes.txt");
    std::fs::write(&src, b"nope").unwrap();
    let mut s = WallpaperSettingsState::load_default_test();
    let res = s.import_custom(&src);
    assert!(matches!(res, ImportResult::Rejected(_)), "txt must be rejected");
    assert!(s.list_custom().is_empty(), "rejected import must not leave a file");
    std::env::remove_var("MULTISOUL_TEST_DATA_ROOT");
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p app test_import_custom 2>&1 | tail -15
```
Expected: FAIL.

- [ ] **Step 3: Implement picker + import**

`file_picker.rs`:
```rust
use std::path::PathBuf;
pub(crate) fn pick_wallpaper_image() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose a wallpaper")
        .add_filter("Images & Video", &["png","jpg","jpeg","gif","mp4","mov"])
        .pick_file()
}
```
`WallpaperSettingsState::import_custom`:
- derive `WallpaperKind::from_extension(src.extension())`; if None → `Rejected("unsupported")`.
- ensure `custom_wallpapers_dir()` exists (create_dir_all).
- compute a non-colliding filename (if exists, append `_2` etc.).
- `fs::copy(src, dest)`; on IO error → `Rejected(err)`.
- `select_custom(filename)` + persist.
- return `Imported(filename)`.

`MultisoulApp::import_wallpaper_file`:
```rust
pub(crate) fn import_wallpaper_file(&mut self, cx: &mut Context<Self>) {
    cx.spawn(async move |this, cx| {
        let picked = cx.background_executor().spawn(async move {
            crate::app::file_picker::pick_wallpaper_image()  // rfd blocks; run off main thread
        }).await;
        let Some(path) = picked else { return };
        this.update(cx, |app, cx| {
            let _ = app.wallpaper_settings.import_custom(&path);
            cx.notify();
        }).ok();
    }).detach();
}
```
(Verify rfd's blocking `pick_file` is fine to call from a background task; if rfd needs the main thread on mac, switch to `rfd::AsyncFileDialog` + `await`. Prefer `AsyncFileDialog` to be safe.)

- [ ] **Step 4: Run to verify it passes + check**

```bash
cargo test -p app test_import_custom 2>&1 | tail -15
cargo check -p app 2>&1 | tail -20
```
Expected: PASS + clean.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/app/file_picker.rs crates/app/src/app/mod.rs crates/app/src/settings/wallpaper.rs crates/app/tests/unit/wallpaper_settings_tests.rs
git commit -m "feat(wallpaper): rfd file picker + custom import/copy flow

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 9: git-lfs for videos + copy all 10 built-ins into assets + packaging script

**Files:**
- Create/modify: `.gitattributes` (root) — `*.mp4 filter=lfs diff=lfs merge=lfs -text`
- Create: `crates/app/assets/wallpapers/{1403512,1404942,1406443,1406489,1407174}.mp4` (via git-lfs)
- Create: `scripts/copy-wallpaper-resources.sh`
- Modify: `docs/references/packaging.md`

**Interfaces:**
- Produces: all 10 built-in files at `crates/app/assets/wallpapers/`; a packaging step that copies `assets/wallpapers/*` → bundle `Contents/Resources/wallpapers/`.

- [ ] **Step 1: Initialize git-lfs for mp4**

```bash
git lfs install   # if not already
printf '%s\n' '*.mp4 filter=lfs diff=lfs merge=lfs -text' >> .gitattributes
git add .gitattributes
```

- [ ] **Step 2: Copy the 5 mp4 sources into assets (they'll be tracked by lfs)**

```bash
cp /Users/alan/Documents/壁纸/mut/1403512.mp4 crates/app/assets/wallpapers/
cp /Users/alan/Documents/壁纸/mut/1404942.mp4 crates/app/assets/wallpapers/
cp /Users/alan/Documents/壁纸/mut/1406443.mp4/assets/wallpapers/ 2>/dev/null || cp /Users/alan/Documents/壁纸/mut/1406443.mp4 crates/app/assets/wallpapers/
cp /Users/alan/Documents/壁纸/mut/1406489.mp4 crates/app/assets/wallpapers/
cp /Users/alan/Documents/壁纸/mut/1407174.mp4 crates/app/assets/wallpapers/
git add crates/app/assets/wallpapers/*.mp4
git lfs ls-files   # confirm tracked as lfs
```

- [ ] **Step 3: Write the packaging helper script**

`scripts/copy-wallpaper-resources.sh`:
```bash
#!/usr/bin/env bash
# 把内置壁纸资源拷进 macOS app bundle 的 Contents/Resources/wallpapers/。
# 用法: copy-wallpaper-resources.sh <bundle_resources_dir>
set -euo pipefail
RES_DIR="${1:?usage: $0 <bundle_resources_dir>}"
SRC_DIR="$(cd "$(dirname "$0")/.." && pwd)/crates/app/assets/wallpapers"
mkdir -p "$RES_DIR/wallpapers"
cp -R "$SRC_DIR/." "$RES_DIR/wallpapers/"
echo "copied wallpapers -> $RES_DIR/wallpapers"
```
`chmod +x scripts/copy-wallpaper-resources.sh`.

- [ ] **Step 4: Document in packaging.md**

Append a "Wallpaper resources" section noting: release packaging must invoke `scripts/copy-wallpaper-resources.sh <app>/Contents/Resources` after `cargo packager` lays out the bundle (or wire into a Packager.toml `resources` glob if a Packager.toml is added — none currently exists, so document the manual step).

- [ ] **Step 5: Verify + commit**

```bash
git lfs ls-files | grep mp4   # 5 entries
bash scripts/check-max-file-lines.sh   # untouched
ls crates/app/assets/wallpapers/ | wc -l   # 10 (9 new + default) — actually 5 png/gif + 5 mp4 + default = wait: default 1331865.png already there
```
Expected: 10 files total in the dir.

```bash
git add .gitattributes scripts/copy-wallpaper-resources.sh docs/references/packaging.md crates/app/assets/wallpapers/*.mp4
git commit -m "feat(wallpaper): bundle all 10 built-ins, mp4 via git-lfs, packaging script

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 10: Video wallpaper render glue (surface() + timer) — GATED by Task 1 spike

> **Prerequisite: Task 1 spike MUST be committed and its `VideoDecoder` interface frozen.** This task consumes that exact interface. If the spike changed any signature, update the interface block below to match before implementing.

**Files:**
- Create: `crates/app/src/app/video_wallpaper.rs` (mac-only)
- Modify: `crates/app/src/app/mod.rs` (`mod video_wallpaper;` mac-gated; field for current `CVPixelBuffer` + decoder; spawn the timer)
- Modify: `crates/app/src/app/chrome.rs` (add the video branch in `render_wallpaper_layer`)

**Interfaces:**
- Consumes: `VideoDecoder`/`DecodedVideoFrame` from Task 1 (frozen interface).
- Produces:
  ```rust
  #[cfg(target_os = "macos")]
  pub(crate) fn render_video_wallpaper(app: &MultisoulApp, cx: &mut Context<MultisoulApp>) -> Div;
  ```

- [ ] **Step 1: Add state fields + a failing compile/anchor test**

`crates/app/tests/unit/app_chrome_tests.rs`:
```rust
/// T-W9: 视频壁纸分支在 chrome.rs 中受 mac 门控存在；surface() 被用于上屏。
#[test]
fn test_video_wallpaper_branch_uses_surface() {
    let src = include_str!("../../src/app/chrome.rs");
    assert!(src.contains("WallpaperKind::Video"), "chrome must route Video kind");
    let vsrc = include_str!("../../src/app/video_wallpaper.rs");
    assert!(vsrc.contains("surface("), "video wallpaper must render via surface()");
    assert!(vsrc.contains("background_executor().timer"), "video must self-advance via timer");
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p app test_video_wallpaper_branch_uses_surface 2>&1 | tail -10
```
Expected: FAIL.

- [ ] **Step 3: Implement `video_wallpaper.rs`**

State on `MultisoulApp` (mac-only; use `Option<...>` fields with `#[cfg]`):
```rust
#[cfg(target_os = "macos")]
video_decoder: Option<VideoDecoder>,
#[cfg(target_os = "macos")]
video_frame: Option<DecodedVideoFrame>,
```
On selection change to a Video kind, `MultisoulApp` opens a decoder for the resolved path (`resolve_path`) and spawns a timer:
```rust
#[cfg(target_os = "macos")]
fn start_video_loop(&mut self, cx: &mut Context<Self>) {
    let path = match self.wallpaper_settings.resolve_path() { Some(p) => p, None => return };
    self.video_decoder = VideoDecoder::open(&path).ok();
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(std::time::Duration::from_millis(40)).await;
            let Ok(()) = this.update(cx, |app, cx| {
                if let Some(dec) = app.video_decoder.as_mut() {
                    if let Ok(frame) = dec.next_frame() {
                        app.video_frame = Some(frame);
                        cx.notify();
                    }
                }
            }) else { break; }
        }
    }).detach();
}
```
Render:
```rust
#[cfg(target_os = "macos")]
pub(crate) fn render_video_wallpaper(app: &MultisoulApp, _cx) -> Div {
    use gpui::{div, prelude::*, px, surface, Div, ObjectFit};
    let mut layer = div().absolute().size_full();
    if let Some(frame) = app.video_frame.as_ref() {
        layer = layer.child(surface(frame.buffer.clone())
            .absolute().top(px(0.0)).left(px(0.0)).size_full()
            .object_fit(ObjectFit::Cover));
    }
    layer
}
```
Call `start_video_loop` whenever selection becomes a Video kind (in `select_builtin`/`select_custom`/`import_custom` commit points → simplest: check kind in `render_wallpaper_layer` and lazily start). Stop the loop / drop decoder when kind changes away from Video.
Add the `#[cfg(target_os="macos")]` branch in `chrome.rs` `render_wallpaper_layer`.

- [ ] **Step 4: Run to verify + native build**

```bash
cargo test -p app test_video_wallpaper_branch_uses_surface 2>&1 | tail -5
cargo check -p app 2>&1 | tail -20
```
Expected: PASS + clean (on macOS).

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/app/video_wallpaper.rs crates/app/src/app/mod.rs crates/app/src/app/chrome.rs crates/app/tests/unit/app_chrome_tests.rs
git commit -m "feat(wallpaper): video wallpaper via AVFoundation + surface() + 25fps timer

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 11: Video thumbnails (first frame) in the grid

**Files:**
- Modify: `crates/app/src/settings/wallpaper_view.rs` (video cell uses `VideoDecoder::first_frame_bgra` → `img(RenderImage)`)
- Consumes: `VideoDecoder::first_frame_bgra` from Task 1.

- [ ] **Step 1: Anchor test**

```rust
/// T-W10: 视频缩略图走 first_frame，不实时解码播放。
#[test]
fn test_video_thumbnail_uses_first_frame() {
    let src = include_str!("../../src/settings/wallpaper_view.rs");
    assert!(src.contains("first_frame_bgra") || src.contains("video_thumb"),
        "video thumbnail must use first_frame");
}
```

- [ ] **Step 2–4: Implement (replace the labeled `div("VIDEO")` placeholder from Task 7 with a real thumbnail), run, verify.**

Generate the first-frame BGRA on selection-list build (cache per file in the state to avoid re-decoding each render). Convert `image::RgbaImage` → `gpui::RenderImage` (RGBA→BGRA swap per img.rs:642). Gate mac-only; on non-mac keep the labeled div.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/settings/wallpaper_view.rs crates/app/tests/unit/settings_panel_tests.rs
git commit -m "feat(wallpaper): video first-frame thumbnails in grid

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Task 12: Full verification

**Files:** none (verification only).

- [ ] **Step 1: Mechanized constraints**

```bash
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
bash scripts/check-storage-boundaries.sh
```
Expected: all pass. Fix any file that exceeded 500 lines by splitting (likely `wallpaper.rs` → already split into `wallpaper_view.rs`; `video_decoder.rs` if large).

- [ ] **Step 2: Domain + workspace tests**

```bash
cargo test -p canvas -p terminal -p runtime -p storage
cargo test --workspace 2>&1 | tail -30
cargo check -p app 2>&1 | tail -20
```
Expected: all pass / clean.

- [ ] **Step 3: Dependency boundary**

```bash
cargo tree -p canvas -p terminal -p runtime | grep -i gpui
```
Expected: no output (domain crates stay GPUI-free — wallpaper code is in `app` only).

- [ ] **Step 4: Manual app run + acceptance**

```bash
cargo run -p app
```
Manually verify against SPEC §11:
- [ ] Settings → Wallpaper shows 10 built-ins + any customs.
- [ ] Select each: static PNG updates background; GIF animates; mp4 loops.
- [ ] Restart → previous selection restored (incl. video/GIF).
- [ ] Import a local png and a local mp4 → appear + work + deletable; built-ins not deletable.
- [ ] Import a `.txt` → rejected, no crash, default stays.
- [ ] Remove the selected custom's file externally + restart → falls back to default with a warn log.
- [ ] `RUST_LOG=trace cargo run -p app` shows fallback `warn`s on broken inputs, no panics.

- [ ] **Step 5: Final commit (any verification fixes)**

```bash
git add -A
git commit -m "test(wallpaper): verification fixes

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Self-Review Checklist (run before declaring done)

- **Spec coverage:** Static/GIF/Video render ✓ (T1,T5,T10); 10 built-ins ✓ (T2,T3,T9); persistence ✓ (T4); Settings UI grid+import+delete ✓ (T6,T7,T8); bundle packaging ✓ (T9); thumbnails ✓ (T11); fallback+warn ✓ (T4); 500-line ✓ (T12); no-allow ✓ (T12); domain GPUI-free ✓ (T12).
- **Placeholder scan:** No "TBD"/"implement later". The only spike-gated interface (Task 1 → 10/11) is written as concrete signatures. Task 7's video thumbnail cell starts as a labeled `div("VIDEO")` deliberately replaced in T11 — documented, not hidden.
- **Type consistency:** `WallpaperKind` (T2), `WallpaperSelection`/`WallpaperSettingsState` (T4), `VideoDecoder`/`DecodedVideoFrame` (T1, used T10/T11) — names match across tasks.
