//! The prototype's Aurora WebGL material, captured as RGBA frames for GPUI.

use std::time::Duration;

use gpui::{div, img, prelude::*, px, Div, Stateful};

// Prototype shader moves its highlight at 0.28 radians per presence second.
const ORBIT_PERIOD_SECONDS: f32 = std::f32::consts::TAU / 0.28;
const ORB_FRAME_COUNT: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbMode {
    Idle,
    Listening,
    Thinking,
    Speaking,
}

impl OrbMode {
    pub(crate) fn target_energy(self) -> f32 {
        match self {
            Self::Idle => 0.1,
            Self::Listening => 0.8,
            Self::Thinking => 0.45,
            Self::Speaking => 0.65,
        }
    }
}

pub(crate) fn advance_orb_phase(phase: f32, delta: Duration, reduce_motion: bool) -> f32 {
    if reduce_motion {
        return phase;
    }
    (phase + delta.as_secs_f32() / ORBIT_PERIOD_SECONDS).rem_euclid(1.0)
}

pub(crate) fn advance_orb_energy(
    energy: f32,
    mode: OrbMode,
    delta: Duration,
    reduce_motion: bool,
) -> f32 {
    let target = mode.target_energy();
    if reduce_motion {
        return target;
    }
    let step = delta.as_secs_f32().clamp(0.0, 0.05);
    target + (energy - target) * (-step / 0.18).exp()
}

pub(crate) fn orb_frames_for_phase(phase: f32) -> (usize, usize, f32) {
    let position = phase.rem_euclid(1.0) * ORB_FRAME_COUNT as f32;
    let first = position.floor() as usize;
    (first, (first + 1) % ORB_FRAME_COUNT, position.fract())
}

pub(super) fn orb_art(phase: f32, energy: f32, reduce_motion: bool) -> Stateful<Div> {
    let (first, second, blend) = orb_frames_for_phase(if reduce_motion { 0.0 } else { phase });
    let active_weight = ((energy - 0.1) / 0.7).clamp(0.0, 1.0);
    let frame = |active: bool, index: usize, opacity: f32| {
        img(format!(
            "icons/norma-orb{}-{index:02}.png",
            if active { "-active" } else { "" }
        ))
        .absolute()
        .inset_0()
        .w(px(144.0))
        .h(px(144.0))
        .opacity(opacity)
    };
    div()
        .id("desktop-assistant-orb-art")
        .debug_selector(|| "desktop-assistant-orb-art".to_string())
        .relative()
        .w(px(144.0))
        .h(px(144.0))
        .child(frame(false, first, 1.0))
        .when(blend > 0.0, |art| art.child(frame(false, second, blend)))
        .when(active_weight > 0.0, |art| {
            art.child(frame(true, first, active_weight))
        })
        .when(active_weight > 0.0 && blend > 0.0, |art| {
            art.child(frame(true, second, active_weight * blend))
        })
}
