use super::{FeatureGuideBlockers, FeatureGuideTopic};
use crate::app::voice::control::VoicePhase;
use crate::app::{MultisoulApp, OpenFeatureGuide};
use gpui::{Context, KeyDownEvent, Window};
use std::time::Duration;

const CANVAS_READY_DELAY: Duration = Duration::from_millis(16);
const PENDING_RETRY_INTERVAL: Duration = Duration::from_millis(250);
const MEDIA_FRAME_INTERVAL: Duration = Duration::from_millis(1_200);
const MEDIA_FRAME_COUNT: usize = 3;

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
                    if self.feature_guide.topic == FeatureGuideTopic::TaskStatus {
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
        self.feature_guide_media_frame = 0;
        self.queue_feature_guide_persistence(cx);
        self.start_feature_guide_media_if_needed(cx);
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
            self.feature_guide_media_frame = 0;
            self.queue_feature_guide_persistence(cx);
            self.start_feature_guide_media_if_needed(cx);
            cx.notify();
        }
    }

    pub(crate) fn sync_feature_guide_blockers(&mut self, cx: &mut Context<Self>) {
        let was_visible = self.feature_guide.is_visible();
        self.feature_guide
            .sync_blockers(self.feature_guide_blockers());
        if was_visible && !self.feature_guide.is_visible() {
            self.feature_guide_media_task = None;
        } else if !was_visible && self.feature_guide.is_visible() {
            self.queue_feature_guide_persistence(cx);
            self.start_feature_guide_media_if_needed(cx);
        }
    }

    pub(crate) fn close_feature_guide_for_session(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.close_for_session();
        self.feature_guide_media_task = None;
        self.queue_feature_guide_persistence(cx);
        cx.notify();
    }

    pub(crate) fn suppress_feature_guide(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.suppress();
        self.feature_guide_media_task = None;
        self.queue_feature_guide_persistence(cx);
        cx.notify();
    }

    pub(crate) fn complete_feature_guide(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.complete();
        self.feature_guide_media_task = None;
        self.queue_feature_guide_persistence(cx);
        cx.notify();
    }

    pub(crate) fn select_feature_guide_topic(
        &mut self,
        topic: FeatureGuideTopic,
        cx: &mut Context<Self>,
    ) {
        self.feature_guide.select_topic(topic);
        self.feature_guide_media_frame = 0;
        self.queue_feature_guide_persistence(cx);
        cx.notify();
    }

    pub(crate) fn next_feature_guide_topic(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.next_topic();
        self.feature_guide_media_frame = 0;
        self.queue_feature_guide_persistence(cx);
        cx.notify();
    }

    pub(crate) fn previous_feature_guide_topic(&mut self, cx: &mut Context<Self>) {
        self.feature_guide.previous_topic();
        self.feature_guide_media_frame = 0;
        self.queue_feature_guide_persistence(cx);
        cx.notify();
    }

    pub(crate) fn restart_feature_guide_demo(&mut self, cx: &mut Context<Self>) {
        self.feature_guide_media_frame = 0;
        self.start_feature_guide_media_if_needed(cx);
        cx.notify();
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

    fn start_feature_guide_media_if_needed(&mut self, cx: &mut Context<Self>) {
        self.feature_guide_media_task = None;
        if self.motion.reduce_motion || !self.feature_guide.is_visible() {
            return;
        }
        let task = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(MEDIA_FRAME_INTERVAL).await;
            let should_continue = this
                .update(cx, |app, cx| {
                    if !app.feature_guide.is_visible() || app.motion.reduce_motion {
                        return false;
                    }
                    app.feature_guide_media_frame =
                        (app.feature_guide_media_frame + 1) % MEDIA_FRAME_COUNT;
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !should_continue {
                break;
            }
        });
        self.feature_guide_media_task = Some(task);
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
