use std::time::{Duration, Instant};

use gpui::{div, prelude::*, Context, Task, Window};

use super::channel::{VoiceWindowInsetEventBus, VoiceWindowInsetSnapshot};
use super::{VoiceWindowInset, VoiceWindowInsetVisual};

pub(crate) const VOICE_ANIMATION_INTERVAL: Duration = Duration::from_millis(12);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VoiceVisualKey {
    alpha: u8,
    inset_sixteenth_px: u16,
}

impl VoiceVisualKey {
    fn from_visual(visual: VoiceWindowInsetVisual) -> Self {
        Self {
            alpha: (visual.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            inset_sixteenth_px: (visual.inset_px.max(0.0) * 16.0).round() as u16,
        }
    }
}

pub(crate) struct VoiceWindowInsetView {
    inset: VoiceWindowInset,
    events: VoiceWindowInsetEventBus,
    frame_pending: bool,
    last_tick: Instant,
    last_presented_key: Option<VoiceVisualKey>,
    _wake_task: Task<()>,
}

impl VoiceWindowInsetView {
    pub(crate) fn new(
        events: VoiceWindowInsetEventBus,
        wake_rx: async_channel::Receiver<()>,
        cx: &mut Context<Self>,
    ) -> Self {
        let wake_task = cx.spawn(async move |this, cx| {
            while wake_rx.recv().await.is_ok() {
                let update = this.update(&mut *cx, |view, cx| {
                    view.drain_events();
                    let next_key = view.current_key();
                    let presentation_changed = next_key != view.last_presented_key;
                    // 活跃动画已经持有下一帧回调；PCM 分片只需入队，避免在两帧之间
                    // 额外触发一次窗口绘制。静止状态则通知一次以启动 Voice driver。
                    if presentation_changed || !view.frame_pending {
                        view.last_presented_key = next_key;
                        cx.notify();
                    }
                });
                if update.is_err() {
                    break;
                }
            }
        });
        Self {
            inset: VoiceWindowInset::default(),
            events,
            frame_pending: false,
            last_tick: cx.background_executor().now(),
            last_presented_key: None,
            _wake_task: wake_task,
        }
    }

    fn drain_events(&mut self) {
        self.inset.set_enabled(self.events.enabled());
        for (source, event) in self.events.drain() {
            self.inset.apply_playback_event(source, event);
        }
        self.publish_state();
    }

    fn publish_state(&self) {
        self.events
            .set_animation_active(self.inset.is_animation_active());
        self.events.publish_snapshot(VoiceWindowInsetSnapshot {
            visual: self.inset.visual(),
            corner_seam_guard_alpha: self.inset.corner_seam_guard_alpha(),
        });
    }

    fn current_key(&self) -> Option<VoiceVisualKey> {
        self.inset.visual().map(VoiceVisualKey::from_visual)
    }

    fn start_frame_driver_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.frame_pending || !self.inset.is_animation_active() {
            return;
        }
        self.last_tick = cx.background_executor().now();
        self.schedule_next_frame(window, cx);
    }

    fn schedule_next_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.frame_pending = true;
        cx.on_next_frame(window, |view, window, cx| {
            view.on_animation_frame(window, cx);
        });
    }

    fn on_animation_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.frame_pending = false;
        self.drain_events();
        if !self.inset.is_animation_active() {
            return;
        }
        let now = cx.background_executor().now();
        let elapsed = now.saturating_duration_since(self.last_tick);
        if elapsed < VOICE_ANIMATION_INTERVAL {
            self.schedule_next_frame(window, cx);
            return;
        }
        self.last_tick = now;
        self.inset.tick(elapsed);
        self.publish_state();
        let next_key = self.current_key();
        if next_key != self.last_presented_key {
            self.last_presented_key = next_key;
            cx.notify();
        }
        if self.inset.is_animation_active() {
            self.schedule_next_frame(window, cx);
        }
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn advance_for_e2e(&mut self, delta: Duration, cx: &mut Context<Self>) {
        self.drain_events();
        self.inset.tick(delta);
        self.publish_state();
        self.last_presented_key = self.current_key();
        cx.notify();
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn reset_clock_for_e2e(&mut self, now: Instant) {
        self.last_tick = now;
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn consume_display_callback_for_e2e(
        &mut self,
        now: Instant,
        cadence: Duration,
        cx: &mut Context<Self>,
    ) -> (bool, bool, bool) {
        if now.saturating_duration_since(self.last_tick) < cadence {
            return (
                true,
                false,
                self.inset.is_animation_active() || self.events.is_animation_active(),
            );
        }
        self.drain_events();
        if !self.inset.is_animation_active() {
            return (false, false, false);
        }
        let delta = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        self.inset.tick(delta);
        self.publish_state();
        let next_key = self.current_key();
        let notified = next_key != self.last_presented_key;
        self.last_presented_key = next_key;
        if notified {
            cx.notify();
        }
        (false, notified, self.inset.is_animation_active())
    }
}

impl gpui::Render for VoiceWindowInsetView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.start_frame_driver_if_needed(window, cx);
        match self.inset.visual() {
            Some(visual) => super::render(visual),
            None => div().id("voice-window-inset-idle").absolute().size_full(),
        }
    }
}
