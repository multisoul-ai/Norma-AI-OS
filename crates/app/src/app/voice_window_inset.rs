use std::collections::VecDeque;

#[cfg(test)]
use audio::PcmAudioChunk;
use gpui::{
    canvas, div, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, Background,
    Bounds, Path, PathBuilder, Pixels, Stateful,
};
use std::time::Duration;

mod channel;
mod events;
mod meter;
mod seam;
mod view;

pub(crate) use channel::{VoiceWindowInsetEventBus, VoiceWindowInsetEventSink};
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

pub(crate) fn render(visual: VoiceWindowInsetVisual) -> Stateful<gpui::Div> {
    div()
        .id("voice-window-inset")
        .debug_selector(|| "voice-window-inset".to_string())
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .child(
            canvas(
                move |bounds, window, _cx| {
                    build_unified_inset_paths(bounds, visual, window.scale_factor())
                },
                move |_bounds, paths, window, _cx| {
                    for (path, background) in paths {
                        window.paint_path(path, background);
                    }
                },
            )
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .size_full(),
        )
}

#[derive(Clone, Copy, Debug)]
enum InsetGradient {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
enum InsetRegion {
    Rect {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        gradient: InsetGradient,
    },
    Triangle {
        a: (f32, f32),
        b: (f32, f32),
        c: (f32, f32),
        gradient: InsetGradient,
    },
}

fn build_unified_inset_paths(
    bounds: Bounds<Pixels>,
    visual: VoiceWindowInsetVisual,
    scale_factor: f32,
) -> Vec<(Path<Pixels>, Background)> {
    let alpha = (visual.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let white = rgba(0xffffff00 | u32::from(alpha));
    let transparent = rgba(0xffffff00);
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);

    let mut paths = inset_partition(width, height, visual.inset_px)
        .into_iter()
        .filter_map(|region| {
            region
                .path(bounds.origin)
                .map(|path| (path, region.gradient().background(white, transparent)))
        })
        .collect::<Vec<_>>();
    paths.extend(seam::build_corner_seam_paths(
        bounds.origin,
        width,
        height,
        visual.inset_px,
        visual.opacity,
        scale_factor,
    ));
    paths
}

fn inset_partition(width: f32, height: f32, requested_inset: f32) -> Vec<InsetRegion> {
    let width = width.max(0.0);
    let height = height.max(0.0);
    let inset = requested_inset.clamp(0.0, width.min(height) * 0.5);
    if inset <= EPSILON {
        return Vec::new();
    }

    let right = width - inset;
    let bottom = height - inset;
    let mut regions = Vec::with_capacity(12);

    if right - inset > EPSILON {
        regions.push(InsetRegion::Rect {
            left: inset,
            top: 0.0,
            right,
            bottom: inset,
            gradient: InsetGradient::Top,
        });
        regions.push(InsetRegion::Rect {
            left: inset,
            top: bottom,
            right,
            bottom: height,
            gradient: InsetGradient::Bottom,
        });
    }
    if bottom - inset > EPSILON {
        regions.push(InsetRegion::Rect {
            left: 0.0,
            top: inset,
            right: inset,
            bottom,
            gradient: InsetGradient::Left,
        });
        regions.push(InsetRegion::Rect {
            left: right,
            top: inset,
            right: width,
            bottom,
            gradient: InsetGradient::Right,
        });
    }

    regions.extend([
        InsetRegion::Triangle {
            a: (0.0, 0.0),
            b: (inset, 0.0),
            c: (inset, inset),
            gradient: InsetGradient::Top,
        },
        InsetRegion::Triangle {
            a: (0.0, 0.0),
            b: (inset, inset),
            c: (0.0, inset),
            gradient: InsetGradient::Left,
        },
        InsetRegion::Triangle {
            a: (right, 0.0),
            b: (width, 0.0),
            c: (right, inset),
            gradient: InsetGradient::Top,
        },
        InsetRegion::Triangle {
            a: (width, 0.0),
            b: (width, inset),
            c: (right, inset),
            gradient: InsetGradient::Right,
        },
        InsetRegion::Triangle {
            a: (right, height),
            b: (right, bottom),
            c: (width, height),
            gradient: InsetGradient::Bottom,
        },
        InsetRegion::Triangle {
            a: (width, height),
            b: (right, bottom),
            c: (width, bottom),
            gradient: InsetGradient::Right,
        },
        InsetRegion::Triangle {
            a: (0.0, height),
            b: (inset, bottom),
            c: (inset, height),
            gradient: InsetGradient::Bottom,
        },
        InsetRegion::Triangle {
            a: (0.0, height),
            b: (0.0, bottom),
            c: (inset, bottom),
            gradient: InsetGradient::Left,
        },
    ]);
    regions
}

impl InsetGradient {
    fn background(self, white: gpui::Rgba, transparent: gpui::Rgba) -> Background {
        let angle = match self {
            Self::Top => 180.0,
            Self::Bottom => 0.0,
            Self::Left => 90.0,
            Self::Right => 270.0,
        };
        linear_gradient(
            angle,
            linear_color_stop(white, 0.0),
            linear_color_stop(transparent, 1.0),
        )
    }
}

impl InsetRegion {
    fn gradient(self) -> InsetGradient {
        match self {
            Self::Rect { gradient, .. } | Self::Triangle { gradient, .. } => gradient,
        }
    }

    fn path(self, origin: gpui::Point<Pixels>) -> Option<Path<Pixels>> {
        let offset_x = f32::from(origin.x);
        let offset_y = f32::from(origin.y);
        let to_point = |(x, y): (f32, f32)| point(px(offset_x + x), px(offset_y + y));
        let mut builder = PathBuilder::fill();
        match self {
            Self::Rect {
                left,
                top,
                right,
                bottom,
                ..
            } => builder.add_polygon(
                &[
                    to_point((left, top)),
                    to_point((right, top)),
                    to_point((right, bottom)),
                    to_point((left, bottom)),
                ],
                true,
            ),
            Self::Triangle { a, b, c, .. } => {
                builder.add_polygon(&[to_point(a), to_point(b), to_point(c)], true)
            }
        }
        builder.build().ok()
    }

    #[cfg(test)]
    fn contains(self, x: f32, y: f32) -> bool {
        match self {
            Self::Rect {
                left,
                top,
                right,
                bottom,
                ..
            } => x >= left && x < right && y >= top && y < bottom,
            Self::Triangle { a, b, c, .. } => {
                let cross = |from: (f32, f32), to: (f32, f32)| {
                    (to.0 - from.0) * (y - from.1) - (to.1 - from.1) * (x - from.0)
                };
                let ab = cross(a, b);
                let bc = cross(b, c);
                let ca = cross(c, a);
                (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
            }
        }
    }
}

#[cfg(test)]
fn inset_partition_coverage(width: f32, height: f32, inset: f32, x: f32, y: f32) -> usize {
    inset_partition(width, height, inset)
        .into_iter()
        .filter(|region| region.contains(x, y))
        .count()
}

#[cfg(test)]
#[path = "../../tests/unit/voice_window_inset_tests.rs"]
mod tests;
