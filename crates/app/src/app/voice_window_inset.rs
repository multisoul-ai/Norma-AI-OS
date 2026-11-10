use std::collections::VecDeque;

#[cfg(test)]
use audio::PcmAudioChunk;
use std::time::Duration;

mod channel;
mod classic;
mod events;
mod meter;
mod seam;
mod view;

pub(crate) use channel::{VoiceWindowInsetEventBus, VoiceWindowInsetEventSink};
#[cfg(test)]
use classic::inset_partition_coverage;
pub(crate) use classic::render;
pub(crate) use events::{TtsPlaybackEvent, TtsPlaybackSource};
use meter::PcmEnergyFrame;
pub(crate) use meter::PcmEnergyTimeline;
use seam::corner_seam_compensation_alpha;
pub(crate) use view::VoiceWindowInsetView;

const QUIET_OPACITY: f32 = 0.12;
const QUIET_INSET_PX: f32 = 30.0;
const PEAK_OPACITY: f32 = 0.55;
const PEAK_INSET_PX: f32 = 38.0;
const FADE_IN: Duration = Duration::from_millis(120);
const NORMAL_FADE_OUT: Duration = Duration::from_millis(220);
const INTERRUPT_FADE_OUT: Duration = Duration::from_millis(80);
const ENERGY_ATTACK: Duration = Duration::from_millis(35);
const ENERGY_RELEASE: Duration = Duration::from_millis(120);
const CORNER_SEAM_DEVICE_PX: f32 = 1.0;
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
    disabled: bool,
    playback_source: Option<TtsPlaybackSource>,
    pending_playbacks: usize,
    intensity: f32,
    transition: Option<Transition>,
    energy_frames: VecDeque<PcmEnergyFrame>,
    active_energy: Option<PcmEnergyFrame>,
    active_energy_remaining: Duration,
    target_energy: f32,
    smoothed_energy: f32,
}

impl VoiceWindowInset {
    pub(crate) fn is_animation_active(&self) -> bool {
        !self.disabled
            && (self.pending_playbacks > 0
                || self.transition.is_some()
                || !self.energy_frames.is_empty()
                || self.active_energy.is_some()
                || self.target_energy > EPSILON
                || self.smoothed_energy > EPSILON)
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) -> bool {
        if self.disabled != enabled {
            return false;
        }
        self.disabled = !enabled;
        if !enabled {
            self.playback_source = None;
            self.pending_playbacks = 0;
            self.intensity = 0.0;
            self.transition = None;
            self.clear_energy();
        }
        true
    }

    pub(crate) fn begin_playback(&mut self) {
        self.pending_playbacks = self.pending_playbacks.saturating_add(1);
        if self.pending_playbacks == 1 {
            self.transition_to(1.0, FADE_IN);
        }
    }

    #[cfg(test)]
    pub(crate) fn queue_pcm_energy(&mut self, chunk: &PcmAudioChunk) {
        self.queue_energy_timeline(PcmEnergyTimeline::from_pcm(chunk));
    }

    pub(crate) fn queue_energy_timeline(&mut self, timeline: PcmEnergyTimeline) {
        self.energy_frames.extend(timeline.into_frames());
    }

    pub(crate) fn finish_playback(&mut self) {
        if self.pending_playbacks == 0 {
            return;
        }
        self.pending_playbacks = self.pending_playbacks.saturating_sub(1);
        if self.pending_playbacks == 0 {
            self.clear_energy();
            self.transition_to(0.0, NORMAL_FADE_OUT);
        }
    }

    pub(crate) fn interrupt_playback(&mut self) {
        self.pending_playbacks = 0;
        self.clear_energy();
        self.transition_to(0.0, INTERRUPT_FADE_OUT);
    }

    pub(crate) fn tick(&mut self, delta: Duration) -> bool {
        if self.disabled {
            return false;
        }
        let transition_changed = self.tick_transition(delta);
        let energy_changed = self.tick_energy(delta);
        transition_changed || energy_changed
    }

    pub(crate) fn visual(&self) -> Option<VoiceWindowInsetVisual> {
        (!self.disabled && self.intensity > EPSILON).then_some(VoiceWindowInsetVisual {
            opacity: lerp(QUIET_OPACITY, PEAK_OPACITY, self.smoothed_energy) * self.intensity,
            inset_px: lerp(QUIET_INSET_PX, PEAK_INSET_PX, self.smoothed_energy) * self.intensity,
        })
    }

    pub(crate) fn corner_seam_guard_alpha(&self) -> f32 {
        self.visual()
            .map(|visual| corner_seam_compensation_alpha(visual.opacity))
            .unwrap_or(0.0)
    }

    fn tick_transition(&mut self, delta: Duration) -> bool {
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

    fn tick_energy(&mut self, delta: Duration) -> bool {
        if self.pending_playbacks > 0 {
            self.consume_energy_frames(delta);
        }
        let smoothing = if self.target_energy > self.smoothed_energy {
            ENERGY_ATTACK
        } else {
            ENERGY_RELEASE
        };
        let blend = 1.0 - (-delta.as_secs_f32() / smoothing.as_secs_f32()).exp();
        let next = self.smoothed_energy + (self.target_energy - self.smoothed_energy) * blend;
        let changed = (next - self.smoothed_energy).abs() > EPSILON;
        self.smoothed_energy = next;
        changed
    }

    fn consume_energy_frames(&mut self, mut remaining: Duration) {
        while !remaining.is_zero() {
            if self.active_energy.is_none() {
                self.active_energy = self.energy_frames.pop_front();
                self.active_energy_remaining = self
                    .active_energy
                    .map(|frame| frame.duration)
                    .unwrap_or(Duration::ZERO);
                self.target_energy = self.active_energy.map(|frame| frame.energy).unwrap_or(0.0);
            }
            if self.active_energy.is_none() {
                break;
            }
            let consumed = remaining.min(self.active_energy_remaining);
            remaining = remaining.saturating_sub(consumed);
            self.active_energy_remaining = self.active_energy_remaining.saturating_sub(consumed);
            if self.active_energy_remaining.is_zero() {
                self.active_energy = None;
            }
        }
    }

    fn clear_energy(&mut self) {
        self.energy_frames.clear();
        self.active_energy = None;
        self.active_energy_remaining = Duration::ZERO;
        self.target_energy = 0.0;
        self.smoothed_energy = 0.0;
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

fn lerp(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount.clamp(0.0, 1.0)
}

#[cfg(test)]
#[path = "../../tests/unit/voice_window_inset_tests.rs"]
mod tests;
