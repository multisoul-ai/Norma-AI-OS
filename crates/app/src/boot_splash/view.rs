//! GPUI glue for the boot animation. Frames come from the shared low-copy video
//! pipeline (background AVAssetReader → 420f IOSurface → `surface()`); the
//! foreground only swaps the latest frame and advances the timeline.

use super::{
    boot_variant_for, claim_cold_start, forced_boot_variant, should_play_boot_splash,
    variant_duration, BootTimeline,
};
use crate::app::video_wallpaper::{
    poll_video_wallpaper, replace_video_wallpaper, video_wallpaper_surface, VideoFrameMailbox,
    VideoWallpaperState,
};
use crate::app::wallpaper_resolvers::resolve_builtin_asset;
use gpui::{div, prelude::*, App, Context, Entity, MouseButton, Subscription, Window};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The boot video is 60fps; poll at display rate only for its ~6s lifetime.
const BOOT_POLL_INTERVAL: Duration = Duration::from_millis(16);

pub(crate) struct BootSplash {
    started: Instant,
    timeline: BootTimeline,
    video: Option<VideoWallpaperState>,
    _keys: Subscription,
}

impl BootSplash {
    /// The animation for the first window of a cold launch, if policy and assets allow it.
    pub(crate) fn for_cold_launch(cx: &mut App) -> Option<Entity<Self>> {
        let e2e_controlled = std::env::var_os("NORMA_E2E_CONTROL_SOCKET").is_some();
        let e2e_value = std::env::var("NORMA_E2E_BOOT_ANIMATION")
            .ok()
            .filter(|value| !value.is_empty());
        let e2e_opt_in = e2e_value.is_some();
        let reduce_motion = crate::motion::system_reduce_motion();
        if !should_play_boot_splash(reduce_motion, e2e_controlled, e2e_opt_in)
            || !claim_cold_start()
        {
            tracing::info!(
                reduce_motion,
                e2e_controlled,
                e2e_opt_in,
                "boot animation skipped"
            );
            return None;
        }
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.subsec_nanos() as u64)
            .unwrap_or(0);
        let asset =
            forced_boot_variant(e2e_value.as_deref()).unwrap_or_else(|| boot_variant_for(seed));
        let Some(path) = resolve_builtin_asset(asset) else {
            tracing::warn!(asset, "boot animation asset missing");
            return None;
        };
        tracing::info!(?path, "boot animation started");
        let length = variant_duration(asset);
        Some(cx.new(|cx| Self::start(path, length, cx)))
    }

    fn start(path: PathBuf, length: Duration, cx: &mut Context<Self>) -> Self {
        let mut video = None;
        let mailbox = replace_video_wallpaper(&mut video, path.clone());
        let keys = cx.observe_keystrokes(|splash, _, _, cx| splash.skip(cx));
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(BOOT_POLL_INTERVAL).await;
            match this.update(cx, |splash, cx| splash.tick(&path, &mailbox, cx)) {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
        })
        .detach();
        Self {
            started: Instant::now(),
            timeline: BootTimeline::with_duration(Duration::ZERO, length),
            video,
            _keys: keys,
        }
    }

    /// Returns whether to keep polling.
    fn tick(
        &mut self,
        path: &PathBuf,
        mailbox: &Arc<VideoFrameMailbox>,
        cx: &mut Context<Self>,
    ) -> bool {
        let now = self.started.elapsed();
        poll_video_wallpaper(&mut self.video, path, mailbox);
        match self.video.as_ref() {
            Some(state) if !state.is_active() => self.timeline.fail(),
            _ if video_wallpaper_surface(self.video.as_ref()).is_some() => {
                self.timeline.on_first_frame(now)
            }
            _ => {}
        }
        self.timeline.advance(now);
        if self.timeline.is_done() {
            tracing::info!(
                elapsed_ms = now.as_millis() as u64,
                "boot animation finished"
            );
            // dropping the slot cancels the decoder thread
            self.video = None;
        }
        cx.notify();
        !self.timeline.is_done()
    }

    fn skip(&mut self, cx: &mut Context<Self>) {
        self.timeline.skip(self.started.elapsed());
        cx.notify();
    }
}

impl Render for BootSplash {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let opacity = self.timeline.opacity(self.started.elapsed());
        if self.timeline.is_done() || opacity <= 0.0 {
            return div().id("boot-splash-done");
        }
        div()
            .id("boot-splash")
            .debug_selector(|| "boot-splash".to_string())
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(gpui::black())
            .opacity(opacity)
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|splash, _, _, cx| splash.skip(cx)),
            )
            .children(video_wallpaper_surface(self.video.as_ref()))
    }
}
