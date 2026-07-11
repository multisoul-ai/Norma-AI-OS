use gpui::{div, linear_color_stop, linear_gradient, prelude::*, px, rgba, Stateful};
use std::time::Duration;

const QUIET_OPACITY: f32 = 0.12;
const QUIET_INSET_PX: f32 = 10.0;
const FADE_IN: Duration = Duration::from_millis(120);
const NORMAL_FADE_OUT: Duration = Duration::from_millis(220);
const INTERRUPT_FADE_OUT: Duration = Duration::from_millis(80);
const EPSILON: f32 = 0.0001;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VoiceWindowInsetVisual {
    pub opacity: f32,
    pub inset_px: f32,
}

#[derive(Clone, Copy, Debug)]
struct Transition {
    from: f32,
    to: f32,
    elapsed: Duration,
    duration: Duration,
}

#[derive(Debug, Default)]
pub(crate) struct VoiceWindowInset {
    pending_playbacks: usize,
    intensity: f32,
    transition: Option<Transition>,
}

impl VoiceWindowInset {
    pub(crate) fn begin_playback(&mut self) {
        self.pending_playbacks = self.pending_playbacks.saturating_add(1);
        if self.pending_playbacks == 1 {
            self.transition_to(1.0, FADE_IN);
        }
    }

    pub(crate) fn finish_playback(&mut self) {
        if self.pending_playbacks == 0 {
            return;
        }
        self.pending_playbacks = self.pending_playbacks.saturating_sub(1);
        if self.pending_playbacks == 0 {
            self.transition_to(0.0, NORMAL_FADE_OUT);
        }
    }

    pub(crate) fn interrupt_playback(&mut self) {
        self.pending_playbacks = 0;
        self.transition_to(0.0, INTERRUPT_FADE_OUT);
    }

    pub(crate) fn tick(&mut self, delta: Duration) -> bool {
        let Some(mut transition) = self.transition else {
            return false;
        };
        transition.elapsed = transition.elapsed.saturating_add(delta);
        let progress = if transition.duration.is_zero() {
            1.0
        } else {
            (transition.elapsed.as_secs_f32() / transition.duration.as_secs_f32()).min(1.0)
        };
        let next = transition.from + (transition.to - transition.from) * progress;
        let changed = (next - self.intensity).abs() > EPSILON;
        self.intensity = next;
        if progress >= 1.0 {
            self.intensity = transition.to;
            self.transition = None;
        } else {
            self.transition = Some(transition);
        }
        changed
    }

    pub(crate) fn visual(&self) -> Option<VoiceWindowInsetVisual> {
        (self.intensity > EPSILON).then_some(VoiceWindowInsetVisual {
            opacity: QUIET_OPACITY * self.intensity,
            inset_px: QUIET_INSET_PX * self.intensity,
        })
    }

    fn transition_to(&mut self, target: f32, duration: Duration) {
        if (self.intensity - target).abs() <= EPSILON {
            self.intensity = target;
            self.transition = None;
            return;
        }
        self.transition = Some(Transition {
            from: self.intensity,
            to: target,
            elapsed: Duration::ZERO,
            duration,
        });
    }
}

pub(crate) fn render(visual: VoiceWindowInsetVisual) -> Stateful<gpui::Div> {
    let alpha = (visual.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let white = rgba(0xffffff00 | u32::from(alpha));
    let transparent = rgba(0xffffff00);
    let inset = px(visual.inset_px);

    div()
        .id("voice-window-inset")
        .debug_selector(|| "voice-window-inset".to_string())
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .h(inset)
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(white, 0.0),
                    linear_color_stop(transparent, 1.0),
                )),
        )
        .child(
            div()
                .absolute()
                .bottom(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .h(inset)
                .bg(linear_gradient(
                    0.0,
                    linear_color_stop(white, 0.0),
                    linear_color_stop(transparent, 1.0),
                )),
        )
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .bottom(px(0.0))
                .left(px(0.0))
                .w(inset)
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(white, 0.0),
                    linear_color_stop(transparent, 1.0),
                )),
        )
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .bottom(px(0.0))
                .right(px(0.0))
                .w(inset)
                .bg(linear_gradient(
                    270.0,
                    linear_color_stop(white, 0.0),
                    linear_color_stop(transparent, 1.0),
                )),
        )
}

#[cfg(test)]
#[path = "../../tests/unit/voice_window_inset_tests.rs"]
mod tests;
