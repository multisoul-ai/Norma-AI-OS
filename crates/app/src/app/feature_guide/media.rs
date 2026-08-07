use super::FeatureGuideTopic;
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use crate::app::video_wallpaper::{video_frame_surface, VideoFrameMailbox, VideoWallpaperState};
#[cfg(target_os = "macos")]
use gpui::Surface;
#[cfg(target_os = "macos")]
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct FeatureGuideMediaState {
    pub(super) topic: Option<FeatureGuideTopic>,
    pub(super) video_path: Option<PathBuf>,
    pub(super) poster_path: Option<PathBuf>,
    pub(super) manually_paused: bool,
    pub(super) playing: bool,
    pub(super) poll_generation: u64,
    #[cfg(target_os = "macos")]
    pub(super) video: Option<VideoWallpaperState>,
    #[cfg(target_os = "macos")]
    pub(super) mailbox: Option<Arc<VideoFrameMailbox>>,
}

impl FeatureGuideMediaState {
    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn is_playing(&self) -> bool {
        self.playing
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn poster_available(&self) -> bool {
        self.poster_path.is_some()
    }

    pub(super) fn clear_playback(&mut self) {
        self.playing = false;
        self.poll_generation = self.poll_generation.wrapping_add(1);
        #[cfg(target_os = "macos")]
        {
            self.video = None;
            self.mailbox = None;
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn video_surface(&self) -> Option<Surface> {
        video_frame_surface(self.video.as_ref())
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn video_surface(&self) -> Option<gpui::Surface> {
        None
    }
}

pub(crate) fn should_play_feature_guide_media(
    visible: bool,
    reduce_motion: bool,
    manually_paused: bool,
) -> bool {
    visible && !reduce_motion && !manually_paused
}
