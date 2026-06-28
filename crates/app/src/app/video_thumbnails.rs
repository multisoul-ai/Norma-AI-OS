//! Video first-frame thumbnail cache for the wallpaper settings grid (Task 11).
//!
//! The grid view ([`crate::settings::wallpaper::view`]) renders one cell per
//! wallpaper. For video cells, `VideoDecoder::first_frame_bgra(path)` decodes a
//! single still frame that serves as the cell thumbnail. Decoding hits disk +
//! AVFoundation, so it must NOT run on every repaint: this module caches the
//! resulting `Arc<RenderImage>` keyed by absolute path and lazily spawns a
//! background decode the first time a path is requested.
//!
//! # Why a `thread_local` cache instead of an app field
//!
//! `crates/app/src/app/mod.rs` is at its 500-line cap; the task brief forbids
//! adding a field to `MultisoulApp`. The wallpaper grid renders exclusively on
//! the GPUI main thread, so a main-thread `thread_local` `RefCell` matches the
//! access pattern without polluting `MultisoulApp` or risking a borrow-window
//! collision.
//!
//! # Byte order (RGBA → BGRA)
//!
//! Despite its name, `first_frame_bgra` returns an `image::RgbaImage` whose
//! bytes are honest **RGBA**. The decoder's `bgra_pixel_buffer_to_rgba` helper
//! reads the 32BGRA CVPixelBuffer as `(b, g, r, a)` and reconstructs
//! `image::Rgba([r, g, b, a])`, so the `RgbaImage` is RGBA-ordered. GPUI's
//! `RenderImage`, however, requires **BGRA** (see `assets.rs`'s "A cached and
//! processed image, in BGRA format" doc and the per-pixel `pixel.swap(0, 2)` in
//! `img.rs`). [`render_image_from_rgba`] performs that swap before building the
//! single GPUI frame, mirroring exactly what GPUI's own image cache does.
//!
//! GATED to macOS: `VideoDecoder::first_frame_bgra` is mac-only.

#![cfg(target_os = "macos")]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Context, RenderImage};
use image::Frame;

use crate::app::video_decoder::VideoDecoder;
use crate::app::MultisoulApp;

thread_local! {
    /// Per-process thumbnail cache. The key is the resolved absolute video path
    /// (same form the decoder is invoked with); the value is the decoded still.
    static THUMBNAILS: std::cell::RefCell<ThumbnailCache> =
        std::cell::RefCell::new(ThumbnailCache::default());
}

#[derive(Default)]
struct ThumbnailCache {
    /// Successfully decoded first frames, ready to render.
    ready: HashMap<PathBuf, Arc<RenderImage>>,
    /// Paths with a background decode already in flight. Prevents re-spawning
    /// the same AVAssetReader on every repaint while the first decode is pending.
    inflight: HashSet<PathBuf>,
}

/// Look up a cached first-frame thumbnail for `path`.
///
/// Returns `None` on the first request (before the decode completes) or when a
/// previous decode failed — callers should then render the labeled fallback and
/// call [`request`] to (re)kick the decode.
pub(crate) fn get(path: &Path) -> Option<Arc<RenderImage>> {
    THUMBNAILS.with(|c| c.borrow().ready.get(path).cloned())
}

/// Spawn a background `first_frame_bgra` decode for `path` if one is not already
/// cached or in flight. On success the result is inserted into the cache and
/// `cx.notify()` is fired so the next repaint picks up the thumbnail. Decode
/// failures are logged and the inflight marker is cleared, leaving the cell on
/// the labeled fallback (no retry storm — the next repaint re-enters via the
/// not-cached branch, but `inflight` dedup still applies within a decode).
pub(crate) fn request(path: PathBuf, cx: &mut Context<MultisoulApp>) {
    let needs_spawn = THUMBNAILS.with(|c| {
        let mut c = c.borrow_mut();
        if c.ready.contains_key(&path) || c.inflight.contains(&path) {
            return false;
        }
        c.inflight.insert(path.clone());
        true
    });
    if !needs_spawn {
        return;
    }

    cx.spawn(async move |this, cx| {
        let decode_path = path.clone();
        let result = cx
            .background_executor()
            .spawn(async move { VideoDecoder::first_frame_bgra(&decode_path) })
            .await;
        match result {
            Ok(rgba) => {
                let image = Arc::new(render_image_from_rgba(rgba));
                THUMBNAILS.with(|c| {
                    let mut c = c.borrow_mut();
                    c.inflight.remove(&path);
                    c.ready.insert(path, image);
                });
                let _ = this.update(cx, |_, cx| cx.notify());
            }
            Err(e) => {
                tracing::warn!(error = %e, "video thumbnail first-frame decode failed");
                THUMBNAILS.with(|c| {
                    c.borrow_mut().inflight.remove(&path);
                });
            }
        }
    })
    .detach();
}

/// Build a non-animating single-frame GPUI `RenderImage` from an RGBA
/// `image::RgbaImage`, performing the RGBA→BGRA swap GPUI requires. See the
/// module-level byte-order note for the rationale.
fn render_image_from_rgba(rgba: image::RgbaImage) -> RenderImage {
    let mut buffer = rgba;
    for pixel in buffer.chunks_exact_mut(4) {
        // RGBA → BGRA: swap the red (index 0) and blue (index 2) channels.
        pixel.swap(0, 2);
    }
    let frame = Frame::new(buffer);
    RenderImage::new(vec![frame])
}
