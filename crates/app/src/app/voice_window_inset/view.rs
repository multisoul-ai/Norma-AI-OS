use std::time::{Duration, Instant};

use gpui::{div, prelude::*, Context, Task, Window};

use super::channel::{VoiceWindowInsetEventBus, VoiceWindowInsetSnapshot};
use super::glow::GlowDriver;
use super::{paint, VoiceWindowInset, VoiceWindowInsetVisual};
use crate::effects::voice_glow::scene::Rect;
use crate::effects::voice_glow::{VoiceGlowPrefs, VoiceGlowStyle};

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
    glow: GlowDriver,
    prefs: VoiceGlowPrefs,
    /// 风格切换后必须再画一帧：旧风格的最后一帧图元否则会一直留在屏幕上。
    needs_repaint: bool,
    viewport: (f32, f32),
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
                    let style_changed = std::mem::take(&mut view.needs_repaint);
                    let presentation_changed = style_changed || next_key != view.last_presented_key;
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
            glow: GlowDriver::default(),
            prefs: VoiceGlowPrefs::default(),
            needs_repaint: false,
            viewport: (1280.0, 800.0),
            events,
            frame_pending: false,
            last_tick: cx.background_executor().now(),
            last_presented_key: None,
            _wake_task: wake_task,
        }
    }

    fn uses_glow(&self) -> bool {
        matches!(
            self.prefs.style,
            VoiceGlowStyle::Halo | VoiceGlowStyle::Ribbon | VoiceGlowStyle::Stardust
        )
    }

    fn animation_active(&self) -> bool {
        self.inset.is_animation_active() || (self.uses_glow() && self.glow.is_active())
    }

    fn tick_glow(&mut self, elapsed: Duration) {
        if !self.uses_glow() {
            return;
        }
        let (width, height) = self.viewport;
        // Dock 探针还没写入时按默认布局估算：底部居中，高 52、距底 22。
        let dock = self.events.dock().unwrap_or_else(|| {
            let w = (width - 36.0).clamp(0.0, 640.0);
            Rect {
                x: (width - w) / 2.0,
                y: height - 74.0,
                w,
                h: 52.0,
            }
        });
        self.glow.tick(
            elapsed,
            self.inset.glow_speaking(),
            self.prefs,
            self.viewport,
            dock,
        );
    }

    fn drain_events(&mut self) {
        self.inset.set_enabled(self.events.enabled());
        let prefs = self.events.prefs();
        if prefs.style != self.prefs.style {
            self.glow.reset();
            self.needs_repaint = true;
        }
        self.prefs = prefs;
        self.glow
            .set_phase(prefs.effective_phase(self.events.phase()));
        for (source, event) in self.events.drain() {
            self.inset.apply_playback_event(source, event);
        }
        self.publish_state();
    }

    fn publish_state(&self) {
        self.events.set_animation_active(self.animation_active());
        self.events.publish_snapshot(VoiceWindowInsetSnapshot {
            visual: self.inset.visual(),
            corner_seam_guard_alpha: self.inset.corner_seam_guard_alpha(),
        });
    }

    fn current_key(&self) -> Option<VoiceVisualKey> {
        self.inset.visual().map(VoiceVisualKey::from_visual)
    }

    fn start_frame_driver_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.frame_pending || !self.animation_active() {
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
        self.viewport = (
            f32::from(window.viewport_size().width),
            f32::from(window.viewport_size().height),
        );
        self.drain_events();
        if std::mem::take(&mut self.needs_repaint) {
            cx.notify();
        }
        if !self.animation_active() {
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
        self.tick_glow(elapsed);
        self.publish_state();
        if self.uses_glow() {
            cx.notify();
        } else {
            let next_key = self.current_key();
            if next_key != self.last_presented_key {
                self.last_presented_key = next_key;
                cx.notify();
            }
        }
        if self.animation_active() {
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
        self.viewport = (
            f32::from(window.viewport_size().width),
            f32::from(window.viewport_size().height),
        );
        self.start_frame_driver_if_needed(window, cx);
        match self.prefs.style {
            VoiceGlowStyle::Off => div().id("voice-window-inset-idle").absolute().size_full(),
            VoiceGlowStyle::Classic => match self.inset.visual() {
                Some(visual) => super::render(visual),
                None => div().id("voice-window-inset-idle").absolute().size_full(),
            },
            _ => paint::render(self.glow.prims()),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_voice_inset_tests.rs"]
mod desktop_tests;
