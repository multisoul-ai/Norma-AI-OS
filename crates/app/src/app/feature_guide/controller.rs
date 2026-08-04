use super::{
    should_play_feature_guide_media, FeatureGuideBlockers, FeatureGuideSettingsTransition,
    FeatureGuideTopic,
};
use crate::app::voice::control::VoicePhase;
use crate::app::wallpaper_resolvers::resolve_builtin_asset;
use crate::app::{MultisoulApp, OpenFeatureGuide};
use crate::settings::panel::SettingsSection;
use gpui::{Context, KeyDownEvent, Window};
use std::time::Duration;

#[cfg(target_os = "macos")]
use crate::app::video_wallpaper::{
    poll_video_wallpaper, replace_video_wallpaper, VideoFrameMailbox,
    VIDEO_FOREGROUND_POLL_INTERVAL_MS,
};
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(target_os = "macos")]
use std::sync::Arc;

const CANVAS_READY_DELAY: Duration = Duration::from_millis(16);
const PENDING_RETRY_INTERVAL: Duration = Duration::from_millis(250);

impl MultisoulApp {
    pub(crate) fn handle_feature_guide_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.feature_guide.is_visible() {
            return false;
        }
        let modifiers = event.keystroke.modifiers;
        if event.keystroke.key == "tab"
            && !modifiers.platform
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.function
        {
            return true;
        }
        if !event.keystroke.modifiers.modified() {
            match event.keystroke.key.as_str() {
                "escape" => self.close_feature_guide_for_session(cx),
                "up" => self.previous_feature_guide_topic(cx),
                "down" => self.next_feature_guide_topic(cx),
                "enter" | "space" if self.feature_guide_primary_focus.is_focused(window) => {
                    if self.feature_guide.topic == FeatureGuideTopic::AgentNotifications {
                        self.complete_feature_guide(cx);
                    } else {
                        self.next_feature_guide_topic(cx);
                    }
                }
                _ => {}
            }
        }
        cx.stop_propagation();
        true
    }

    pub(crate) fn on_open_feature_guide(
        &mut self,
        _: &OpenFeatureGuide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.feature_guide.open_manually();
        self.feature_guide
            .sync_blockers(self.feature_guide_blockers());
        self.feature_guide_media.manually_paused = false;
        self.queue_feature_guide_persistence(cx);
        self.sync_feature_guide_media(cx);
        if self.feature_guide.is_visible() {
            self.focus_handle.focus(window);
        }
        cx.notify();
        cx.stop_propagation();
    }

    pub(crate) fn schedule_feature_guide_after_workspace_open(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.on_workspace_opened();
        if !self.feature_guide.is_pending() {
            return;
        }
        let expected_root = self.workspace_state.current_root.clone();
        cx.spawn(async move |this, cx| {
            // 1000ms / 60Hz ≈ 16.67ms；等待 16ms 让 Canvas 首帧先进入 GPUI 渲染队列。
            cx.background_executor().timer(CANVAS_READY_DELAY).await;
            loop {
                let keep_waiting = this
                    .update(cx, |app, cx| {
                        if app.workspace_state.current_root != expected_root {
                            return false;
                        }
                        app.present_feature_guide_if_ready(cx);
                        app.feature_guide.is_pending()
                    })
                    .unwrap_or(false);
                if !keep_waiting {
                    break;
                }
                // 250ms = 每秒 4 次低频资格复查；只在 Pending 阶段存在，不进入逐帧热路径。
                cx.background_executor().timer(PENDING_RETRY_INTERVAL).await;
            }
        })
        .detach();
    }

    pub(crate) fn present_feature_guide_if_ready(&mut self, cx: &mut Context<Self>) {
        if !self.window_active {
            return;
        }
        let was_visible = self.feature_guide.is_visible();
        self.feature_guide
            .on_canvas_ready(self.feature_guide_blockers());
        if self.feature_guide.is_visible() && !was_visible {
            self.feature_guide_media.manually_paused = false;
            self.queue_feature_guide_persistence(cx);
            self.sync_feature_guide_media(cx);
            cx.notify();
        }
    }

    pub(crate) fn sync_feature_guide_blockers(&mut self, cx: &mut Context<Self>) {
        let was_visible = self.feature_guide.is_visible();
        self.feature_guide
            .sync_blockers(self.feature_guide_blockers());
        if was_visible && !self.feature_guide.is_visible() {
            self.pause_feature_guide_media(false, cx);
        } else if !was_visible && self.feature_guide.is_visible() {
            self.queue_feature_guide_persistence(cx);
            self.sync_feature_guide_media(cx);
        }
    }

    pub(crate) fn close_feature_guide_for_session(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.close_for_session();
        self.feature_guide_media.clear_playback();
        self.queue_feature_guide_persistence(cx);
        cx.notify();
    }

    pub(crate) fn suppress_feature_guide(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.suppress();
        self.feature_guide_media.clear_playback();
        self.queue_feature_guide_persistence(cx);
        cx.notify();
    }

    pub(crate) fn complete_feature_guide(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.complete();
        self.feature_guide_media.clear_playback();
        self.queue_feature_guide_persistence(cx);
        cx.notify();
    }

    pub(crate) fn select_feature_guide_topic(
        &mut self,
        topic: FeatureGuideTopic,
        cx: &mut Context<Self>,
    ) {
        self.feature_guide.select_topic(topic);
        self.queue_feature_guide_persistence(cx);
        self.sync_feature_guide_media(cx);
        cx.notify();
    }

    pub(crate) fn next_feature_guide_topic(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.next_topic();
        self.queue_feature_guide_persistence(cx);
        self.sync_feature_guide_media(cx);
        cx.notify();
    }

    pub(crate) fn previous_feature_guide_topic(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.previous_topic();
        self.queue_feature_guide_persistence(cx);
        self.sync_feature_guide_media(cx);
        cx.notify();
    }

    pub(crate) fn toggle_feature_guide_demo_playback(&mut self, cx: &mut Context<Self>) {
        if self.motion.reduce_motion {
            return;
        }
        if self.feature_guide_media.playing {
            self.pause_feature_guide_media(true, cx);
        } else {
            self.feature_guide_media.manually_paused = false;
            self.sync_feature_guide_media(cx);
        }
        cx.notify();
    }

    pub(crate) fn toggle_feature_guide_orchestration_details(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.toggle_orchestration_details();
        cx.notify();
    }

    pub(crate) fn open_feature_guide_orchestration_settings(&mut self, cx: &mut Context<Self>) {
        if self.motion.reduce_motion {
            self.begin_feature_guide_settings_transition(
                FeatureGuideSettingsTransition::Opening,
                cx,
            );
        }
        self.settings_panel.open();
        self.settings_panel.select(SettingsSection::Orchestration);
        self.sync_feature_guide_blockers(cx);
        self.reconcile_orchestration_integrations(cx);
        cx.notify();
    }

    pub(crate) fn begin_feature_guide_settings_transition(
        &mut self,
        transition: FeatureGuideSettingsTransition,
        cx: &mut Context<Self>,
    ) {
        let generation = self.feature_guide.begin_settings_transition(transition);
        let duration = self.motion.reduced_fade_duration();
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            let _ = this.update(cx, |app, cx| {
                if !app.feature_guide.finish_settings_transition(generation) {
                    return;
                }
                if transition == FeatureGuideSettingsTransition::Closing {
                    app.finish_settings_close_after_feature_guide_crossfade(cx);
                } else {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn feature_guide_blockers(&self) -> FeatureGuideBlockers {
        FeatureGuideBlockers {
            settings_open: self.settings_panel.open,
            voice_active: matches!(
                self.voice.phase(),
                VoicePhase::Recording { .. } | VoicePhase::Transcribing
            ) || self.voice_caption_is_visible(),
            confirmation_open: self.pending_agent_close_confirmation.is_some(),
            workspace_error_visible: self.workspace_state.error.is_some(),
        }
    }

    pub(in crate::app) fn sync_feature_guide_media(&mut self, cx: &mut Context<Self>) {
        let topic = self.feature_guide.topic;
        if self.feature_guide_media.topic != Some(topic) {
            self.feature_guide_media.clear_playback();
            self.feature_guide_media.topic = Some(topic);
            self.feature_guide_media.video_path =
                topic.video_asset().and_then(resolve_builtin_asset);
            self.feature_guide_media.poster_path =
                topic.poster_asset().and_then(resolve_builtin_asset);
            self.feature_guide_media.manually_paused = false;
        }

        if !should_play_feature_guide_media(
            self.feature_guide.is_visible(),
            self.motion.reduce_motion,
            self.feature_guide_media.manually_paused,
        ) {
            self.pause_feature_guide_media(false, cx);
            return;
        }

        if self.feature_guide_media.video_path.is_none() {
            self.pause_feature_guide_media(false, cx);
            return;
        }

        #[cfg(target_os = "macos")]
        self.start_or_resume_feature_guide_video(cx);
        #[cfg(not(target_os = "macos"))]
        {
            self.feature_guide_media.playing = false;
        }
    }

    fn pause_feature_guide_media(&mut self, manually: bool, cx: &mut Context<Self>) {
        if manually {
            self.feature_guide_media.manually_paused = true;
        }
        self.feature_guide_media.playing = false;
        self.feature_guide_media.poll_generation =
            self.feature_guide_media.poll_generation.wrapping_add(1);
        #[cfg(target_os = "macos")]
        if let Some(video) = self.feature_guide_media.video.as_ref() {
            video.set_paused(true);
        }
        cx.notify();
    }

    #[cfg(target_os = "macos")]
    fn start_or_resume_feature_guide_video(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.feature_guide_media.video_path.clone() else {
            self.feature_guide_media.clear_playback();
            return;
        };

        if self.feature_guide_media.playing
            && self
                .feature_guide_media
                .video
                .as_ref()
                .is_some_and(|video| video.is_active() && video.path() == path)
        {
            return;
        }

        let resumable = self
            .feature_guide_media
            .video
            .as_ref()
            .is_some_and(|video| video.is_active() && video.path() == path)
            && self.feature_guide_media.mailbox.is_some();
        let mailbox = if resumable {
            let video = self
                .feature_guide_media
                .video
                .as_ref()
                .expect("resumable feature guide media must retain its video state");
            video.set_paused(false);
            self.feature_guide_media
                .mailbox
                .as_ref()
                .expect("resumable feature guide media must retain its mailbox")
                .clone()
        } else {
            let mailbox =
                replace_video_wallpaper(&mut self.feature_guide_media.video, path.clone());
            self.feature_guide_media.mailbox = Some(mailbox.clone());
            mailbox
        };

        self.feature_guide_media.playing = true;
        self.feature_guide_media.poll_generation =
            self.feature_guide_media.poll_generation.wrapping_add(1);
        let generation = self.feature_guide_media.poll_generation;
        self.spawn_feature_guide_video_poll_loop(path, mailbox, generation, cx);
        cx.notify();
    }

    #[cfg(target_os = "macos")]
    fn spawn_feature_guide_video_poll_loop(
        &mut self,
        path: std::path::PathBuf,
        mailbox: Arc<VideoFrameMailbox>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(VIDEO_FOREGROUND_POLL_INTERVAL_MS))
                .await;
            match this.update(cx, |app, cx| {
                app.poll_feature_guide_video(&path, &mailbox, generation, cx)
            }) {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    fn poll_feature_guide_video(
        &mut self,
        path: &Path,
        mailbox: &Arc<VideoFrameMailbox>,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.feature_guide_media.playing
            || self.feature_guide_media.poll_generation != generation
        {
            return false;
        }
        let (keep_polling, changed) =
            poll_video_wallpaper(&mut self.feature_guide_media.video, path, mailbox);
        if !keep_polling {
            self.feature_guide_media.playing = false;
        }
        if changed {
            cx.notify();
        }
        keep_polling
    }

    fn queue_feature_guide_persistence(&mut self, cx: &mut Context<Self>) {
        self.feature_guide_settings.queue(self.feature_guide);
        self.persist_feature_guide_in_background(cx);
    }

    fn persist_feature_guide_in_background(&mut self, cx: &mut Context<Self>) {
        let Some((store, changes)) = self.feature_guide_settings.take_pending_write() else {
            return;
        };
        let work = cx.background_executor().spawn(async move {
            store
                .apply_string_changes(&changes)
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.feature_guide_settings.finish_write();
                if let Err(error) = result {
                    tracing::warn!(error, "failed to persist feature guide state");
                }
                app.persist_feature_guide_in_background(cx);
            });
        })
        .detach();
    }
}
