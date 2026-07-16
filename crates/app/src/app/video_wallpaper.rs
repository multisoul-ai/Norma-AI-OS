//! Video wallpaper render glue: holds the latest 420f CVPixelBuffer on
//! `MultisoulApp`, decodes frames on a presentation-timestamp-paced background
//! worker, and drives a ~25fps foreground poll loop that swaps the latest frame
//! and triggers `cx.notify()` repaints. The chrome wallpaper layer renders the
//! current buffer via GPUI `surface()`.
//!
//! Live video must stay on this low-copy surface path. The CPU image-frame path
//! is reserved for settings thumbnails, where only a single cached first frame
//! is decoded.

use super::MultisoulApp;
use crate::app::video_decoder::{DecodedVideoFrame, VideoDecoder};
use crate::app::wallpaper_resolvers::WallpaperKind;
use gpui::{div, prelude::*, px, surface, Context, Div, ObjectFit, Surface};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

/// ~25fps foreground cadence for picking up the latest decoded frame.
const VIDEO_FOREGROUND_POLL_INTERVAL_MS: u64 = 40;

/// Playback cadence used only when decoded media does not provide PTS.
const VIDEO_PTS_FALLBACK_INTERVAL_MS: u64 = 40;

/// Back off transient decoder errors without coupling successful frames to a
/// fixed cadence.
const VIDEO_DECODE_ERROR_RETRY_MS: u64 = 40;

/// Prevent malformed or discontinuous media timestamps from stalling the
/// decoder worker for an unbounded period.
const VIDEO_MAX_PRESENTATION_DELAY_MS: u64 = 250;

/// Max consecutive frame errors before we settle the slot to Failed.
const VIDEO_DECODE_MAX_CONSECUTIVE_ERRORS: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoSlotKind {
    Active,
    Failed,
}

pub(crate) enum VideoWallpaperState {
    Active {
        frame: Option<DecodedVideoFrame>,
        mailbox: Arc<VideoFrameMailbox>,
        cancel: Arc<AtomicBool>,
        path: PathBuf,
    },
    Failed {
        path: PathBuf,
    },
}

impl VideoWallpaperState {
    pub(crate) fn path(&self) -> &Path {
        match self {
            VideoWallpaperState::Active { path, .. } | VideoWallpaperState::Failed { path } => path,
        }
    }

    fn kind(&self) -> VideoSlotKind {
        match self {
            VideoWallpaperState::Active { .. } => VideoSlotKind::Active,
            VideoWallpaperState::Failed { .. } => VideoSlotKind::Failed,
        }
    }

    fn cancel(&self) {
        if let VideoWallpaperState::Active { cancel, .. } = self {
            cancel.store(true, Ordering::Release);
        }
    }

    fn worker_parts(&self) -> Option<(PathBuf, Arc<VideoFrameMailbox>, Arc<AtomicBool>)> {
        match self {
            VideoWallpaperState::Active {
                path,
                mailbox,
                cancel,
                ..
            } => Some((path.clone(), mailbox.clone(), cancel.clone())),
            VideoWallpaperState::Failed { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VideoSyncAction {
    Keep,
    Clear,
    Restart { path: PathBuf },
}

#[derive(Default)]
pub(crate) struct VideoFrameMailbox {
    inner: Mutex<VideoFrameMailboxState>,
}

#[derive(Default)]
struct VideoFrameMailboxState {
    latest_frame: Option<DecodedVideoFrame>,
    failure: Option<String>,
}

enum VideoMailboxUpdate {
    None,
    Frame(DecodedVideoFrame),
    Failed(String),
}

impl VideoFrameMailbox {
    fn push_frame(&self, frame: DecodedVideoFrame) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.latest_frame = Some(frame);
        }
    }

    fn fail(&self, error: String) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.failure = Some(error);
        }
    }

    fn take_update(&self) -> VideoMailboxUpdate {
        let Ok(mut inner) = self.inner.lock() else {
            return VideoMailboxUpdate::Failed("video frame mailbox lock poisoned".to_string());
        };
        if let Some(error) = inner.failure.take() {
            return VideoMailboxUpdate::Failed(error);
        }
        if let Some(frame) = inner.latest_frame.take() {
            return VideoMailboxUpdate::Frame(frame);
        }
        VideoMailboxUpdate::None
    }
}

pub(crate) fn slot_path_matches(current: &Path, spawned: &Path) -> bool {
    current == spawned
}

fn video_frame_delay(
    previous_pts: Option<Duration>,
    current_pts: Option<Duration>,
    elapsed_since_previous_frame: Duration,
) -> Duration {
    let nominal_interval = match (previous_pts, current_pts) {
        (Some(previous), Some(current)) if current > previous => current - previous,
        (None, Some(_)) => Duration::ZERO,
        (Some(_), Some(_)) => Duration::ZERO,
        _ => Duration::from_millis(VIDEO_PTS_FALLBACK_INTERVAL_MS),
    };
    nominal_interval
        .saturating_sub(elapsed_since_previous_frame)
        .min(Duration::from_millis(VIDEO_MAX_PRESENTATION_DELAY_MS))
}

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

    let Some(resolved) = resolved else {
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
    /// Sync Canvas video before rendering its backdrop so an outgoing
    /// Launchpad frame cannot appear during the screen transition.
    pub(super) fn sync_canvas_video_wallpaper(&mut self, cx: &mut Context<Self>) {
        let canvas_video = (self.wallpaper_settings.current_kind() == WallpaperKind::Video)
            .then(|| self.wallpaper_settings.resolve_path())
            .flatten();
        self.sync_video_wallpaper(canvas_video.as_deref(), cx);
    }

    /// Render the Launchpad-owned video layer. Its decoder path is chosen by
    /// `render_launchpad`, independently from `WallpaperSettingsState`.
    pub(crate) fn render_launchpad_video_layer(&self) -> Div {
        let host = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .size_full()
            .overflow_hidden();

        match self.render_video_wallpaper_surface() {
            Some(frame) => host.child(frame),
            None => host,
        }
    }

    pub(super) fn render_video_wallpaper_surface(&self) -> Option<Surface> {
        let state = self.video_wallpaper.as_ref()?;
        let frame = match state {
            VideoWallpaperState::Active { frame, .. } => frame.as_ref()?,
            VideoWallpaperState::Failed { .. } => return None,
        };
        Some(
            surface(frame.buffer().clone())
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
    }

    /// Render one clipped card-backdrop sample from the same live 420f buffer.
    /// Multiple calls only clone the CVPixelBuffer handle; no CPU RGBA frame is
    /// allocated, keeping the video wallpaper on GPUI's low-copy surface path.
    pub(super) fn render_video_wallpaper_frost_sample(
        &self,
        left: f32,
        top: f32,
        width: f32,
        height: f32,
        opacity: f32,
    ) -> Option<Surface> {
        let state = self.video_wallpaper.as_ref()?;
        let frame = match state {
            VideoWallpaperState::Active { frame, .. } => frame.as_ref()?,
            VideoWallpaperState::Failed { .. } => return None,
        };
        Some(
            surface(frame.buffer().clone())
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(width))
                .h(px(height))
                .object_fit(ObjectFit::Cover)
                .opacity(opacity),
        )
    }

    pub(crate) fn sync_video_wallpaper(
        &mut self,
        video_path: Option<&Path>,
        cx: &mut Context<Self>,
    ) {
        match decide_video_sync(
            self.video_wallpaper.as_ref(),
            video_path.is_some(),
            video_path,
        ) {
            VideoSyncAction::Keep => {}
            VideoSyncAction::Clear => {
                if let Some(state) = self.video_wallpaper.take() {
                    state.cancel();
                }
                cx.notify();
            }
            VideoSyncAction::Restart { path } => {
                self.start_video_loop(path, cx);
            }
        }
    }

    fn start_video_loop(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(state) = self.video_wallpaper.take() {
            state.cancel();
        }

        let mailbox = Arc::new(VideoFrameMailbox::default());
        let cancel = Arc::new(AtomicBool::new(false));
        self.video_wallpaper = Some(VideoWallpaperState::Active {
            frame: None,
            mailbox: mailbox.clone(),
            cancel: cancel.clone(),
            path: path.clone(),
        });
        cx.notify();

        let decode_path = path.clone();
        let decode_mailbox = mailbox.clone();
        let decode_cancel = cancel.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("multisoul-video-wallpaper-decoder".to_string())
            .spawn(move || {
                decode_video_wallpaper_frames(decode_path, decode_mailbox, decode_cancel);
            })
        {
            mailbox.fail(format!(
                "failed to spawn video wallpaper decoder thread: {error}"
            ));
        }
        self.spawn_video_frame_poll_loop(path, cx);
    }

    fn spawn_video_frame_poll_loop(&mut self, my_path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(VIDEO_FOREGROUND_POLL_INTERVAL_MS))
                .await;
            match this.update(cx, |app, cx| app.poll_video_wallpaper_update(&my_path, cx)) {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
        })
        .detach();
    }

    fn poll_video_wallpaper_update(&mut self, my_path: &Path, cx: &mut Context<Self>) -> bool {
        let Some(state) = self.video_wallpaper.as_ref() else {
            return false;
        };
        if !slot_path_matches(state.path(), my_path) {
            return false;
        }

        let Some((path, mailbox, cancel)) = state.worker_parts() else {
            return false;
        };
        match mailbox.take_update() {
            VideoMailboxUpdate::None => true,
            VideoMailboxUpdate::Frame(new_frame) => {
                self.video_wallpaper = Some(VideoWallpaperState::Active {
                    frame: Some(new_frame),
                    mailbox,
                    cancel,
                    path,
                });
                cx.notify();
                true
            }
            VideoMailboxUpdate::Failed(error) => {
                tracing::warn!(%error, ?path, "video wallpaper decode failed");
                cancel.store(true, Ordering::Release);
                self.video_wallpaper = Some(VideoWallpaperState::Failed { path });
                cx.notify();
                false
            }
        }
    }
}

fn decode_video_wallpaper_frames(
    path: PathBuf,
    mailbox: Arc<VideoFrameMailbox>,
    cancel: Arc<AtomicBool>,
) {
    let mut decoder = match VideoDecoder::open(&path) {
        Ok(decoder) => decoder,
        Err(error) => {
            mailbox.fail(error.to_string());
            return;
        }
    };

    let mut consecutive_errors = 0;
    let mut previous_pts = None;
    let mut previous_presented_at: Option<Instant> = None;
    while !cancel.load(Ordering::Acquire) {
        match decoder.next_frame() {
            Ok(frame) => {
                consecutive_errors = 0;
                let current_pts = frame.presentation_time();
                let elapsed_since_previous_frame = previous_presented_at
                    .map(|presented_at| presented_at.elapsed())
                    .unwrap_or(Duration::ZERO);
                let delay =
                    video_frame_delay(previous_pts, current_pts, elapsed_since_previous_frame);
                if !delay.is_zero() {
                    std::thread::sleep(delay);
                }
                if cancel.load(Ordering::Acquire) {
                    return;
                }
                mailbox.push_frame(frame);
                previous_pts = current_pts;
                previous_presented_at = Some(Instant::now());
            }
            Err(error) => {
                consecutive_errors += 1;
                tracing::warn!(
                    error = %error,
                    consecutive_errors,
                    "video wallpaper decode error"
                );
                if consecutive_errors >= VIDEO_DECODE_MAX_CONSECUTIVE_ERRORS {
                    mailbox.fail(error.to_string());
                    return;
                }
                std::thread::sleep(Duration::from_millis(VIDEO_DECODE_ERROR_RETRY_MS));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/video_wallpaper_tests.rs"]
mod tests;
