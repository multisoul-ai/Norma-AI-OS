//! Shared low-copy video render glue: a PTS-paced worker publishes 420f
//! CVPixelBuffers, and a ~25fps foreground poll swaps the latest frame into a
//! GPUI `surface()`. CPU image frames remain limited to cached thumbnails.

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
pub(crate) const VIDEO_FOREGROUND_POLL_INTERVAL_MS: u64 = 40;

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
        paused: Arc<AtomicBool>,
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

    pub(crate) fn is_active(&self) -> bool {
        matches!(self, VideoWallpaperState::Active { .. })
    }

    pub(crate) fn set_paused(&self, is_paused: bool) {
        if let VideoWallpaperState::Active { paused, .. } = self {
            paused.store(is_paused, Ordering::Release);
        }
    }
}

impl Drop for VideoWallpaperState {
    fn drop(&mut self) {
        self.cancel();
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

pub(crate) fn slot_poll_owner_matches(
    current_path: &Path,
    spawned_path: &Path,
    current_mailbox: &Arc<VideoFrameMailbox>,
    spawned_mailbox: &Arc<VideoFrameMailbox>,
) -> bool {
    slot_path_matches(current_path, spawned_path) && Arc::ptr_eq(current_mailbox, spawned_mailbox)
}

pub(crate) fn video_wallpaper_path_for_feature_guide(
    video_path: Option<&Path>,
    feature_guide_visible: bool,
) -> Option<&Path> {
    (!feature_guide_visible).then_some(video_path).flatten()
}

pub(crate) fn video_wallpaper_should_notify(changed: bool, settings_open: bool) -> bool {
    changed && !settings_open
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

/// Build a layout-free, low-copy GPUI surface from the latest 420f frame.
pub(crate) fn video_frame_surface(state: Option<&VideoWallpaperState>) -> Option<Surface> {
    let frame = match state? {
        VideoWallpaperState::Active { frame, .. } => frame.as_ref()?,
        VideoWallpaperState::Failed { .. } => return None,
    };
    Some(surface(frame.buffer().clone()))
}

/// Render the latest live wallpaper frame through the shared low-copy path.
pub(crate) fn video_wallpaper_surface(state: Option<&VideoWallpaperState>) -> Option<Surface> {
    video_frame_surface(state).map(|frame| {
        frame
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .size_full()
            .object_fit(ObjectFit::Cover)
    })
}

/// Replace a host's current video slot and start the shared AVAssetReader
/// decoder worker. Dropping the previous state signals its worker to stop.
pub(crate) fn replace_video_wallpaper(
    state: &mut Option<VideoWallpaperState>,
    path: PathBuf,
) -> Arc<VideoFrameMailbox> {
    let mailbox = Arc::new(VideoFrameMailbox::default());
    let cancel = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    *state = Some(VideoWallpaperState::Active {
        frame: None,
        mailbox: mailbox.clone(),
        cancel: cancel.clone(),
        paused: paused.clone(),
        path: path.clone(),
    });

    let decode_mailbox = mailbox.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("multisoul-video-wallpaper-decoder".to_string())
        .spawn(move || {
            decode_video_wallpaper_frames(path, decode_mailbox, cancel, paused);
        })
    {
        mailbox.fail(format!(
            "failed to spawn video wallpaper decoder thread: {error}"
        ));
    }
    mailbox
}

/// Pull one mailbox update into a host-owned slot.
/// Returns `(keep_polling, state_changed)` so each GPUI entity can own its
/// foreground timer while sharing all decoder and state-transition logic.
pub(crate) fn poll_video_wallpaper(
    state: &mut Option<VideoWallpaperState>,
    spawned_path: &Path,
    spawned_mailbox: &Arc<VideoFrameMailbox>,
) -> (bool, bool) {
    let Some(current) = state.as_ref() else {
        return (false, false);
    };
    let update = match current {
        VideoWallpaperState::Active { mailbox, .. }
            if slot_poll_owner_matches(current.path(), spawned_path, mailbox, spawned_mailbox) =>
        {
            mailbox.take_update()
        }
        VideoWallpaperState::Active { .. } => return (false, false),
        VideoWallpaperState::Failed { .. } => return (false, false),
    };

    match update {
        VideoMailboxUpdate::None => (true, false),
        VideoMailboxUpdate::Frame(new_frame) => {
            if let Some(VideoWallpaperState::Active { frame, .. }) = state.as_mut() {
                *frame = Some(new_frame);
            }
            (true, true)
        }
        VideoMailboxUpdate::Failed(error) => {
            let path = current.path().to_path_buf();
            tracing::warn!(%error, ?path, "video wallpaper decode failed");
            *state = Some(VideoWallpaperState::Failed { path });
            (false, true)
        }
    }
}

impl MultisoulApp {
    /// Sync Canvas video before rendering its backdrop so an outgoing
    /// Launchpad frame cannot appear during the screen transition.
    pub(crate) fn sync_canvas_video_wallpaper(&mut self, cx: &mut Context<Self>) {
        let canvas_video = (self.wallpaper_settings.current_kind() == WallpaperKind::Video)
            .then(|| self.wallpaper_settings.resolve_path())
            .flatten();
        let visible_video = video_wallpaper_path_for_feature_guide(
            canvas_video.as_deref(),
            self.feature_guide.is_visible(),
        );
        self.sync_video_wallpaper(visible_video, cx);
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
        video_wallpaper_surface(self.video_wallpaper.as_ref())
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
        let mailbox = replace_video_wallpaper(&mut self.video_wallpaper, path.clone());
        cx.notify();
        self.spawn_video_frame_poll_loop(path, mailbox, cx);
    }

    fn spawn_video_frame_poll_loop(
        &mut self,
        my_path: PathBuf,
        my_mailbox: Arc<VideoFrameMailbox>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(VIDEO_FOREGROUND_POLL_INTERVAL_MS))
                .await;
            match this.update(cx, |app, cx| {
                app.poll_video_wallpaper_update(&my_path, &my_mailbox, cx)
            }) {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
        })
        .detach();
    }

    fn poll_video_wallpaper_update(
        &mut self,
        my_path: &Path,
        my_mailbox: &Arc<VideoFrameMailbox>,
        cx: &mut Context<Self>,
    ) -> bool {
        let (keep_polling, changed) =
            poll_video_wallpaper(&mut self.video_wallpaper, my_path, my_mailbox);
        if video_wallpaper_should_notify(changed, self.settings_panel.open) {
            cx.notify();
        }
        keep_polling
    }
}

fn decode_video_wallpaper_frames(
    path: PathBuf,
    mailbox: Arc<VideoFrameMailbox>,
    cancel: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
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
        if paused.load(Ordering::Acquire) {
            previous_presented_at = None;
            std::thread::sleep(Duration::from_millis(VIDEO_FOREGROUND_POLL_INTERVAL_MS));
            continue;
        }
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
