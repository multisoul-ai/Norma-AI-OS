//! Video wallpaper render glue (Task 10): holds the AVAssetReader decoder plus
//! the current 420f CVPixelBuffer on `MultisoulApp` and drives a ~25fps timer
//! loop that advances frames and triggers `cx.notify()` repaints. The chrome
//! wallpaper layer renders the current buffer via GPUI `surface()`.
//!
//! GATED to macOS: AVFoundation / CoreVideo / GPUI `surface()` are mac-only.
//! On non-mac this module is not compiled and `MultisoulApp.video_wallpaper`
//! does not exist; all access sites gate with `#[cfg(target_os = "macos")]`.

#![cfg(target_os = "macos")]

use super::{MultisoulApp, WALLPAPER_IMAGE_OPACITY};
use crate::app::video_decoder::{DecodedVideoFrame, VideoDecoder};
use crate::app::wallpaper_resolvers::WallpaperKind;
use gpui::{prelude::*, px, surface, Context, ObjectFit, Surface};
use std::time::Duration;

/// ~25fps frame-advance interval. The decoder loops at EOF on its own; this
/// timer only paces repaint cadence. Task 11 may refine to media timestamps.
const VIDEO_FRAME_INTERVAL_MS: u64 = 40;

/// Max consecutive `next_frame` errors before we give up and drop the decoder.
///
/// This hardens against the spike's known deferred issue: a video asset with
/// no decodable frames (e.g. an empty / corrupt mp4) would otherwise spin
/// forever, logging an error every 40ms. After this many consecutive errors
/// the loop stops the decoder so the chrome falls back to the default image.
const VIDEO_DECODE_MAX_CONSECUTIVE_ERRORS: u32 = 8;

/// Live video wallpaper state held on `MultisoulApp` while a Video kind is
/// selected. `None` (i.e. `MultisoulApp.video_wallpaper == None`) means no
/// decoder is running and the chrome wallpaper layer renders the default
/// image fallback.
pub(crate) struct VideoWallpaperState {
    pub(crate) decoder: VideoDecoder,
    pub(crate) frame: Option<DecodedVideoFrame>,
}

impl MultisoulApp {
    /// Build the GPUI `Surface` element for the current decoded video frame,
    /// or `None` if no frame has been decoded yet. The chrome wallpaper layer
    /// falls back to the default image `img()` when this returns `None`.
    pub(super) fn render_video_wallpaper_surface(&self) -> Option<Surface> {
        let state = self.video_wallpaper.as_ref()?;
        let frame = state.frame.as_ref()?;
        Some(
            surface(frame.buffer.clone())
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .size_full()
                .object_fit(ObjectFit::Cover)
                .opacity(WALLPAPER_IMAGE_OPACITY),
        )
    }

    /// Synchronize the video decoder lifecycle to the current wallpaper kind.
    ///
    /// Start the decode loop when the selection is Video and no decoder is
    /// running; drop the decoder (the running task notices on its next tick
    /// and exits) when the selection moves away from Video.
    ///
    /// Call this once per render from the chrome layer (which holds
    /// `&mut self` + `cx`). Idempotent: a no-op when the lifecycle already
    /// matches the current kind.
    pub(super) fn sync_video_wallpaper(&mut self, cx: &mut Context<Self>) {
        let is_video = self.wallpaper_settings.current_kind() == WallpaperKind::Video;
        if is_video {
            if self.video_wallpaper.is_none() {
                self.start_video_loop(cx);
            }
        } else if self.video_wallpaper.is_some() {
            // Drop the decoder; the running task sees kind != Video on its
            // next tick, clears state, and exits.
            self.video_wallpaper = None;
            cx.notify();
        }
    }

    /// Open the decoder for the resolved video path and spawn the frame loop.
    /// No-op (warns) when the path can't be resolved so the chrome falls back
    /// to the default image.
    fn start_video_loop(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.wallpaper_settings.resolve_path() else {
            tracing::warn!("video wallpaper selected but path unresolvable; not starting loop");
            return;
        };
        match VideoDecoder::open(&path) {
            Ok(decoder) => {
                self.video_wallpaper = Some(VideoWallpaperState {
                    decoder,
                    frame: None,
                });
                self.spawn_video_frame_loop(cx);
            }
            Err(e) => {
                tracing::error!(error = %e, ?path, "failed to open video wallpaper decoder");
            }
        }
    }

    /// Spawn the ~25fps timer loop that advances the decoder and notifies the
    /// view to repaint. The loop owns no decoder state — it borrows
    /// `app.video_wallpaper` each tick — and exits when the view is released,
    /// the kind switches away from Video, or the error cap is hit.
    fn spawn_video_frame_loop(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut consecutive_errors: u32 = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(VIDEO_FRAME_INTERVAL_MS))
                    .await;
                let keep_going = this.update(cx, |app, cx| -> bool {
                    // Kind switched away from Video, or decoder dropped
                    // elsewhere: clear state and exit the loop.
                    if app.wallpaper_settings.current_kind() != WallpaperKind::Video {
                        app.video_wallpaper = None;
                        return false;
                    }
                    let Some(state) = app.video_wallpaper.as_mut() else {
                        return false; // decoder dropped elsewhere
                    };
                    match state.decoder.next_frame() {
                        Ok(frame) => {
                            consecutive_errors = 0;
                            state.frame = Some(frame);
                            cx.notify();
                            true
                        }
                        Err(e) => {
                            consecutive_errors += 1;
                            tracing::warn!(
                                error = %e,
                                consecutive_errors,
                                "video wallpaper decode error"
                            );
                            if consecutive_errors >= VIDEO_DECODE_MAX_CONSECUTIVE_ERRORS {
                                tracing::error!(
                                    consecutive_errors,
                                    "video wallpaper exceeded max consecutive decode errors; \
                                     stopping loop"
                                );
                                app.video_wallpaper = None;
                                cx.notify();
                                false
                            } else {
                                true
                            }
                        }
                    }
                });
                match keep_going {
                    Ok(true) => {}
                    _ => break, // view released (Err) or loop signalled stop (Ok(false))
                }
            }
        })
        .detach();
    }
}
