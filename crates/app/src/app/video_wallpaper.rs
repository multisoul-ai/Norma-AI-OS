//! Video wallpaper render glue: starts AVFoundation decoding on a background
//! executor and keeps only the latest decoded image frame on the GPUI thread.
//!
//! The foreground render path must stay cheap. It may start/stop a worker and
//! swap an already-decoded `RenderImage`, but it must not open AVFoundation
//! readers or advance video frames.

use super::{MultisoulApp, WALLPAPER_IMAGE_OPACITY};
use crate::app::video_decoder::VideoDecoder;
use crate::app::video_thumbnails::render_image_from_rgba;
use crate::app::wallpaper_resolvers::WallpaperKind;
use gpui::{img, prelude::*, px, Context, Img, ObjectFit, RenderImage, Stateful};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

/// ~25fps frame cadence. The background worker sleeps between decoded frames;
/// the foreground poll task uses the same interval to pick up the latest frame.
const VIDEO_FRAME_INTERVAL_MS: u64 = 40;

/// Max consecutive frame errors before the worker settles the slot to Failed.
const VIDEO_DECODE_MAX_CONSECUTIVE_ERRORS: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VideoSlotKind {
    Loading,
    Playing,
    Failed,
}

pub(crate) enum VideoWallpaperState {
    Loading {
        path: PathBuf,
        mailbox: Arc<VideoFrameMailbox>,
        cancel: Arc<AtomicBool>,
    },
    Playing {
        path: PathBuf,
        mailbox: Arc<VideoFrameMailbox>,
        cancel: Arc<AtomicBool>,
        frame: Arc<RenderImage>,
    },
    Failed {
        path: PathBuf,
    },
}

impl VideoWallpaperState {
    pub(crate) fn path(&self) -> &Path {
        match self {
            VideoWallpaperState::Loading { path, .. }
            | VideoWallpaperState::Playing { path, .. }
            | VideoWallpaperState::Failed { path } => path,
        }
    }

    fn kind(&self) -> VideoSlotKind {
        match self {
            VideoWallpaperState::Loading { .. } => VideoSlotKind::Loading,
            VideoWallpaperState::Playing { .. } => VideoSlotKind::Playing,
            VideoWallpaperState::Failed { .. } => VideoSlotKind::Failed,
        }
    }

    fn cancel(&self) {
        match self {
            VideoWallpaperState::Loading { cancel, .. }
            | VideoWallpaperState::Playing { cancel, .. } => {
                cancel.store(true, Ordering::Release);
            }
            VideoWallpaperState::Failed { .. } => {}
        }
    }

    fn worker_parts(&self) -> Option<(PathBuf, Arc<VideoFrameMailbox>, Arc<AtomicBool>)> {
        match self {
            VideoWallpaperState::Loading {
                path,
                mailbox,
                cancel,
            }
            | VideoWallpaperState::Playing {
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
    latest_frame: Option<Arc<RenderImage>>,
    failure: Option<String>,
}

enum VideoMailboxUpdate {
    None,
    Frame(Arc<RenderImage>),
    Failed(String),
}

impl VideoFrameMailbox {
    fn push_frame(&self, frame: Arc<RenderImage>) {
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
        (VideoSlotKind::Loading | VideoSlotKind::Playing, true) => VideoSyncAction::Keep,
        (VideoSlotKind::Loading | VideoSlotKind::Playing, false) => VideoSyncAction::Restart {
            path: resolved.to_path_buf(),
        },
        (VideoSlotKind::Failed, true) => VideoSyncAction::Keep,
        (VideoSlotKind::Failed, false) => VideoSyncAction::Restart {
            path: resolved.to_path_buf(),
        },
    }
}

impl MultisoulApp {
    pub(super) fn render_video_wallpaper_image(&self) -> Option<Stateful<Img>> {
        let state = self.video_wallpaper.as_ref()?;
        let frame = match state {
            VideoWallpaperState::Playing { frame, .. } => frame.clone(),
            VideoWallpaperState::Loading { .. } | VideoWallpaperState::Failed { .. } => {
                return None;
            }
        };
        Some(
            img(frame)
                .id("video-wallpaper-frame")
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

    pub(crate) fn sync_video_wallpaper(&mut self, cx: &mut Context<Self>) {
        let is_video = self.wallpaper_settings.current_kind() == WallpaperKind::Video;
        let resolved = self.wallpaper_settings.resolve_path();
        match decide_video_sync(self.video_wallpaper.as_ref(), is_video, resolved.as_deref()) {
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
        self.video_wallpaper = Some(VideoWallpaperState::Loading {
            path: path.clone(),
            mailbox: mailbox.clone(),
            cancel: cancel.clone(),
        });
        cx.notify();

        let decode_path = path.clone();
        let decode_mailbox = mailbox.clone();
        let decode_cancel = cancel.clone();
        cx.background_executor()
            .spawn(async move {
                decode_video_wallpaper_frames(decode_path, decode_mailbox, decode_cancel);
            })
            .detach();
        self.spawn_video_frame_poll_loop(path, cx);
    }

    fn spawn_video_frame_poll_loop(&mut self, my_path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(VIDEO_FRAME_INTERVAL_MS))
                .await;
            match this.update(cx, |app, cx| app.poll_video_wallpaper_update(&my_path, cx)) {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
        })
        .detach();
    }

    fn poll_video_wallpaper_update(&mut self, my_path: &Path, cx: &mut Context<Self>) -> bool {
        if self.wallpaper_settings.current_kind() != WallpaperKind::Video {
            if let Some(state) = self.video_wallpaper.take() {
                state.cancel();
                cx.notify();
            }
            return false;
        }

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
            VideoMailboxUpdate::Frame(frame) => {
                self.video_wallpaper = Some(VideoWallpaperState::Playing {
                    path,
                    mailbox,
                    cancel,
                    frame,
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
    let mut decoder = match VideoDecoder::open_bgra(&path) {
        Ok(decoder) => decoder,
        Err(error) => {
            mailbox.fail(error.to_string());
            return;
        }
    };

    let mut consecutive_errors = 0;
    while !cancel.load(Ordering::Acquire) {
        match decoder.next_frame_rgba() {
            Ok(rgba) => {
                consecutive_errors = 0;
                mailbox.push_frame(Arc::new(render_image_from_rgba(rgba)));
            }
            Err(error) => {
                consecutive_errors += 1;
                if consecutive_errors >= VIDEO_DECODE_MAX_CONSECUTIVE_ERRORS {
                    mailbox.fail(error.to_string());
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(VIDEO_FRAME_INTERVAL_MS));
    }
}

#[cfg(test)]
#[path = "../../tests/unit/video_wallpaper_tests.rs"]
mod tests;
