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
use std::path::{Path, PathBuf};
use std::time::Duration;

/// ~25fps frame-advance interval. The decoder loops at EOF on its own; this
/// timer only paces repaint cadence. Task 11 may refine to media timestamps.
const VIDEO_FRAME_INTERVAL_MS: u64 = 40;

/// Max consecutive `next_frame` errors before we give up and settle to the
/// `Failed` state.
///
/// This hardens against the spike's known deferred issue: a video asset with
/// no decodable frames (e.g. an empty / corrupt mp4) would otherwise spin
/// forever, logging an error every 40ms. After this many consecutive errors
/// the loop transitions the slot to [`VideoWallpaperState::Failed`] so the
/// chrome falls back to the default image AND `sync_video_wallpaper` stops
/// retrying. The slot stays `Failed` until the resolved path changes (the user
/// picked a different video) or the kind moves away from Video.
const VIDEO_DECODE_MAX_CONSECUTIVE_ERRORS: u32 = 8;

/// Discriminant of [`VideoWallpaperState`], decoupled from the mac-only
/// `VideoDecoder` so the [`decide_video_sync`] state-machine helper can be
/// unit-tested without constructing a real decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoSlotKind {
    /// Decoder open + running; a frame may or may not be decoded yet.
    Active,
    /// Open failed OR decode loop hit the error cap. Sticky: do not retry
    /// until the resolved path changes.
    Failed,
}

/// Live video wallpaper state held on `MultisoulApp` while a Video kind is
/// selected. `None` (i.e. `MultisoulApp.video_wallpaper == None`) means no
/// attempt has been made yet (or the kind is not Video) — the chrome wallpaper
/// layer renders the default image fallback.
///
/// `Some(Failed { .. })` is the **settled failure** state: `sync_video_wallpaper`
/// must NOT call `start_video_loop` again for the same path, so we do not spin
/// at ~3Hz reopening a corrupt-but-openable asset. Failure clears only when
/// the resolved path changes (user re-selected video) or the kind moves away.
pub(crate) enum VideoWallpaperState {
    /// Decoder is open and the frame loop is advancing.
    Active {
        decoder: VideoDecoder,
        frame: Option<DecodedVideoFrame>,
        /// Path this decoder was opened for; used to detect path changes that
        /// warrant a restart (both of a running `Active` slot and a stuck
        /// `Failed` slot).
        path: PathBuf,
    },
    /// Sticky failure for `path`. See type-level docs.
    Failed { path: PathBuf },
}

impl VideoWallpaperState {
    /// The resolved path this slot was opened for / failed on.
    pub(crate) fn path(&self) -> &Path {
        match self {
            VideoWallpaperState::Active { path, .. } => path,
            VideoWallpaperState::Failed { path } => path,
        }
    }

    /// Discriminant without the heavy payload, for the pure sync helper.
    fn kind(&self) -> VideoSlotKind {
        match self {
            VideoWallpaperState::Active { .. } => VideoSlotKind::Active,
            VideoWallpaperState::Failed { .. } => VideoSlotKind::Failed,
        }
    }
}

/// What `sync_video_wallpaper` should do this tick, given the current slot and
/// the runtime inputs. Pure so the decision (especially the "Failed prevents
/// retry for the same path" invariant) can be unit-tested without GPUI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VideoSyncAction {
    /// Nothing to do — lifecycle already matches the current kind/path.
    Keep,
    /// Kind moved away from Video: drop the slot entirely.
    Clear,
    /// (Re)start the decode loop for `path` (slot was None, Failed-for-a-
    /// different-path, or Active-for-a-different-path).
    Restart { path: PathBuf },
}

/// Pure path-identity check used by the spawned frame loop to detect that it
/// has been superseded. Returns `true` iff `current` (the slot's live path)
/// equals `spawned` (the path this task was spawned for). Kept as a free
/// function so the invariant — "a task exits when the slot's path no longer
/// matches the path it was spawned for" — is unit-testable without GPUI.
pub(crate) fn slot_path_matches(current: &Path, spawned: &Path) -> bool {
    current == spawned
}

/// Decide the sync action purely from the current slot + runtime inputs.
///
/// Invariants enforced (see [`VideoWallpaperState`] docs):
/// - `is_video == false` AND slot present → `Clear` (handles both Active and Failed).
/// - `is_video == true` AND slot `None` → `Restart`.
/// - `is_video == true` AND slot `Failed` for the SAME path → `Keep` (NO retry → no spin).
/// - `is_video == true` AND slot `Failed` for a DIFFERENT path → `Restart` (user re-selected).
/// - `is_video == true` AND slot `Active` for the SAME path → `Keep` (already running).
/// - `is_video == true` AND slot `Active` for a DIFFERENT path → `Restart` (path changed while running).
/// - `is_video == true` AND resolved path is `None` → `Keep` (unresolvable: warn once at start, don't spin).
pub(crate) fn decide_video_sync(
    slot: Option<&VideoWallpaperState>,
    is_video: bool,
    resolved: Option<&Path>,
) -> VideoSyncAction {
    if !is_video {
        return if slot.is_some() {
            VideoSyncAction::Clear
        } else {
            VideoSyncAction::Keep
        };
    }

    // is_video == true from here.
    let Some(resolved) = resolved else {
        // Can't start without a path; don't spin trying.
        return VideoSyncAction::Keep;
    };
    let Some(state) = slot else {
        return VideoSyncAction::Restart {
            path: resolved.to_path_buf(),
        };
    };

    let same_path = state.path() == resolved;
    match (state.kind(), same_path) {
        (VideoSlotKind::Active, true) => VideoSyncAction::Keep,
        (VideoSlotKind::Active, false) => VideoSyncAction::Restart {
            path: resolved.to_path_buf(),
        },
        (VideoSlotKind::Failed, true) => VideoSyncAction::Keep,
        (VideoSlotKind::Failed, false) => VideoSyncAction::Restart {
            path: resolved.to_path_buf(),
        },
    }
}

impl MultisoulApp {
    /// Build the GPUI `Surface` element for the current decoded video frame,
    /// or `None` if no frame has been decoded yet (or the slot is `Failed`).
    /// The chrome wallpaper layer falls back to the default image `img()` when
    /// this returns `None`.
    pub(super) fn render_video_wallpaper_surface(&self) -> Option<Surface> {
        let state = self.video_wallpaper.as_ref()?;
        let frame = match state {
            VideoWallpaperState::Active { frame, .. } => frame.as_ref()?,
            VideoWallpaperState::Failed { .. } => return None,
        };
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
    /// Delegates the restart/clear/keep decision to the pure
    /// [`decide_video_sync`] helper so the "Failed slot does not retry for the
    /// same path" invariant is testable without GPUI.
    ///
    /// Call this once per render from the chrome layer (which holds
    /// `&mut self` + `cx`). Idempotent: a no-op when the lifecycle already
    /// matches the current kind/path.
    pub(super) fn sync_video_wallpaper(&mut self, cx: &mut Context<Self>) {
        let is_video = self.wallpaper_settings.current_kind() == WallpaperKind::Video;
        let resolved = self.wallpaper_settings.resolve_path();
        let action =
            decide_video_sync(self.video_wallpaper.as_ref(), is_video, resolved.as_deref());
        match action {
            VideoSyncAction::Keep => {}
            VideoSyncAction::Clear => {
                self.video_wallpaper = None;
                cx.notify();
            }
            VideoSyncAction::Restart { path } => {
                self.start_video_loop(path, cx);
            }
        }
    }

    /// Open the decoder for `path` and spawn the frame loop. On open failure
    /// the slot transitions to [`VideoWallpaperState::Failed`] (not `None`) so
    /// `sync_video_wallpaper`'s `is_none()`-based restart guard stays FALSE
    /// and we do not spin retrying `open()`. The one-time `tracing::error!`
    /// fires here, not per-render.
    fn start_video_loop(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match VideoDecoder::open(&path) {
            Ok(decoder) => {
                self.video_wallpaper = Some(VideoWallpaperState::Active {
                    decoder,
                    frame: None,
                    path,
                });
                self.spawn_video_frame_loop(cx);
            }
            Err(e) => {
                tracing::error!(
                    error = %e, ?path,
                    "failed to open video wallpaper decoder; settling to Failed so the default image is used without retry"
                );
                self.video_wallpaper = Some(VideoWallpaperState::Failed { path });
                cx.notify();
            }
        }
    }

    /// Spawn the ~25fps timer loop that advances the decoder and notifies the
    /// view to repaint. The loop owns no decoder state — it borrows
    /// `app.video_wallpaper` each tick — and exits when the view is released,
    /// the kind switches away from Video, or the error cap is hit (in which
    /// case the slot is left as `Failed`, not `None`, so the render-side sync
    /// does not retry).
    ///
    /// **Path-identity guard (Task 10 regression fix):** this task captures the
    /// path of the `Active` slot it was spawned for and exits (returns false)
    /// on the first tick where the slot's current path no longer matches. This
    /// is what makes a video A→B switch safe: `sync_video_wallpaper` returns
    /// `Restart{B}`, `start_video_loop` overwrites the slot with `Active{B}`
    /// and spawns task B; the OLD task A observes `slot.path == B != my_path`
    /// on its next 40ms tick and exits WITHOUT touching decoder B. Without
    /// this guard, task A would keep running and call `next_frame()` on
    /// decoder B (concurrent reads + ~25fps double-advance), and every switch
    /// would leak another decoder-driving task. The invariant: **only the
    /// newest task drives the decoder for its captured path.**
    fn spawn_video_frame_loop(&mut self, cx: &mut Context<Self>) {
        // Capture the path this task is responsible for. The slot was just set
        // to `Active { path, .. }` by `start_video_loop`, so read it back here.
        // If, defensively, the slot is no longer Active (cleared between the
        // set and the spawn), there is nothing for this task to drive.
        let my_path = match &self.video_wallpaper {
            Some(VideoWallpaperState::Active { path, .. }) => path.clone(),
            _ => {
                tracing::warn!(
                    "spawn_video_frame_loop called without an Active slot; skipping spawn"
                );
                return;
            }
        };
        cx.spawn(async move |this, cx| {
            let mut consecutive_errors: u32 = 0;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(VIDEO_FRAME_INTERVAL_MS))
                    .await;
                let keep_going = this.update(cx, |app, cx| -> bool {
                    // Kind switched away from Video: clear state and exit.
                    if app.wallpaper_settings.current_kind() != WallpaperKind::Video {
                        app.video_wallpaper = None;
                        return false;
                    }
                    let Some(state) = app.video_wallpaper.as_mut() else {
                        return false; // decoder dropped elsewhere
                    };
                    let VideoWallpaperState::Active {
                        decoder,
                        frame,
                        path,
                    } = state
                    else {
                        // Failed slot shouldn't have a running loop; exit.
                        return false;
                    };
                    // Path-identity guard: if the slot's path no longer matches
                    // the path this task was spawned for, a newer task has taken
                    // ownership (video A→B switch). Exit without touching the
                    // decoder so only the newest task drives it.
                    if !slot_path_matches(path, &my_path) {
                        return false;
                    }
                    match decoder.next_frame() {
                        Ok(new_frame) => {
                            consecutive_errors = 0;
                            *frame = Some(new_frame);
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
                                    max = VIDEO_DECODE_MAX_CONSECUTIVE_ERRORS,
                                    ?path,
                                    "video wallpaper exceeded max consecutive decode errors; \
                                     settling to Failed (no retry until path changes)"
                                );
                                let path = path.clone();
                                app.video_wallpaper = Some(VideoWallpaperState::Failed { path });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Helper: build a Failed slot for `p` without touching VideoDecoder.
    fn failed_slot(p: &str) -> VideoWallpaperState {
        VideoWallpaperState::Failed {
            path: PathBuf::from(p),
        }
    }

    /// T-F1: a Failed slot for the SAME resolved path must NOT trigger a
    /// restart — this is the spin-prevention invariant. Without it,
    /// `sync_video_wallpaper` would call `start_video_loop` every render,
    /// reopening a corrupt-but-openable asset ~3Hz forever.
    #[test]
    fn test_failed_slot_same_path_does_not_restart() {
        let slot = failed_slot("/corrupt.mp4");
        let resolved = Path::new("/corrupt.mp4");
        let action = decide_video_sync(Some(&slot), true, Some(resolved));
        assert_eq!(
            action,
            VideoSyncAction::Keep,
            "Failed slot for the same path must stay Keep — otherwise the system spins reopening a corrupt asset"
        );
    }

    /// T-F2: a Failed slot for a DIFFERENT resolved path (user re-selected a
    /// new video) MUST restart — failure is path-scoped, not global.
    #[test]
    fn test_failed_slot_different_path_restarts() {
        let slot = failed_slot("/old.mp4");
        let resolved = Path::new("/new.mp4");
        let action = decide_video_sync(Some(&slot), true, Some(resolved));
        assert_eq!(
            action,
            VideoSyncAction::Restart {
                path: PathBuf::from("/new.mp4")
            },
            "Failed slot for a different path must Restart — failure must not outlive the offending asset"
        );
    }

    /// T-F3: when kind moves away from Video, a Failed slot must still be
    /// Cleared (so the next time the user picks Video, the slot is None and a
    /// fresh attempt happens).
    #[test]
    fn test_failed_slot_cleared_when_kind_not_video() {
        let slot = failed_slot("/corrupt.mp4");
        let action = decide_video_sync(Some(&slot), false, None);
        assert_eq!(
            action,
            VideoSyncAction::Clear,
            "Leaving Video kind must Clear even a Failed slot, so re-entering Video re-attempts from None"
        );
    }

    /// T-F4: a None slot with Video kind + resolvable path must Restart.
    #[test]
    fn test_none_slot_video_kind_restarts() {
        let action = decide_video_sync(None, true, Some(Path::new("/clip.mp4")));
        assert_eq!(
            action,
            VideoSyncAction::Restart {
                path: PathBuf::from("/clip.mp4")
            },
            "None slot under Video + resolvable path must Restart to start the decode loop"
        );
    }

    /// T-F5: None slot + Video kind + UNresolvable path must Keep (no spin).
    #[test]
    fn test_none_slot_unresolvable_path_keeps() {
        let action = decide_video_sync(None, true, None);
        assert_eq!(
            action,
            VideoSyncAction::Keep,
            "Unresolvable path must not Restart — otherwise resolve_path()==None spins forever"
        );
    }

    /// T-F6: Clear invariant — None slot under non-Video kind must Keep.
    #[test]
    fn test_none_slot_non_video_keeps() {
        let action = decide_video_sync(None, false, None);
        assert_eq!(
            action,
            VideoSyncAction::Keep,
            "None slot under non-Video kind must be a no-op"
        );
    }

    /// T-G1: the path-identity guard returns TRUE when the slot's current path
    /// equals the path the task was spawned for. This is the steady-state
    /// condition under which the task is allowed to keep advancing the decoder.
    ///
    /// 执行过程：
    ///   1. spawned = /a.mp4, current = /a.mp4
    ///   2. slot_path_matches → true → task keeps driving.
    ///
    /// 预期结果：
    ///   - matches == true: same path means the task is still the rightful owner.
    #[test]
    fn test_slot_path_matches_same_path() {
        let spawned = Path::new("/a.mp4");
        let current = Path::new("/a.mp4");
        assert!(
            slot_path_matches(current, spawned),
            "same path must match — the task is still the rightful decoder owner"
        );
    }

    /// T-G2: the path-identity guard returns FALSE when the slot's current path
    /// differs from the path the task was spawned for. This is the
    /// supersession condition (video A→B switch): the old task MUST exit so
    /// only the newest task drives decoder B.
    ///
    /// 执行过程：
    ///   1. spawned = /a.mp4, current = /b.mp4
    ///   2. slot_path_matches → false → old task exits on next 40ms tick.
    ///
    /// 预期结果：
    ///   - matches == false: a different path means this task has been
    ///     superseded and must not touch the (now different) decoder.
    #[test]
    fn test_slot_path_matches_different_path() {
        let spawned = Path::new("/a.mp4");
        let current = Path::new("/b.mp4");
        assert!(
            !slot_path_matches(current, spawned),
            "different path must NOT match — the old task has been superseded by a newer spawn and must exit"
        );
    }
}
