//! Bloub avatar engine for the island: a procedural blob with a face, ported from Bloub (MIT,
//! github.com/NorbertBodziony/bloub). Pure functions of time, no GPUI, no allocation per frame.
//!
//! The body is a radial profile r(theta); eyes are capsules painted on a sphere and projected
//! orthographically. Everything is in units of the resting ball radius (centre at the origin,
//! y down); the renderer scales and translates.
mod expressions;
mod profiles;

use expressions::{expression, Expression, ExpressionId, RESTING};
use profiles::{EGG, HEXAGON, PROFILE_SAMPLES, TRIANGLE};
use std::f32::consts::TAU;
use std::sync::OnceLock;

#[cfg(test)]
#[path = "../../tests/unit/island_bloub.rs"]
mod tests;

/// Body outline points; the 64 profile samples are decimated, Catmull-Rom keeps it smooth.
pub const BODY_POINTS: usize = PROFILE_SAMPLES / 2;
/// Points of one eye capsule outline.
pub const EYE_POINTS: usize = 20;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mood {
    #[default]
    Idle,
    Working,
    Waiting,
    Failed,
    Responded,
}

impl Mood {
    /// Busy moods redraw at a higher rate; every mood still animates (blink, gaze, breathing).
    pub fn is_busy(self) -> bool {
        self == Mood::Working
    }

    fn motion(self) -> Motion {
        let calm = Motion {
            face: None,
            breathe: 0.025,
            period: 3.4,
            wander: 2.,
            sway: 0.,
            bounce: 0.,
            droop: 0.,
        };
        match self {
            Mood::Idle => calm,
            Mood::Working => Motion {
                face: Some(ExpressionId::Attentive),
                breathe: 0.06,
                period: 0.9,
                wander: 3.,
                ..calm
            },
            Mood::Waiting => Motion {
                face: Some(ExpressionId::Curious),
                sway: 12.,
                ..calm
            },
            Mood::Failed => Motion {
                face: Some(ExpressionId::Sad),
                droop: 0.07,
                wander: 0.6,
                ..calm
            },
            Mood::Responded => Motion {
                face: Some(ExpressionId::Happy),
                bounce: 0.14,
                period: 1.1,
                ..calm
            },
        }
    }
}

/// How a mood moves, on top of the face it wears. Gaze drift is exaggerated against Bloub's own
/// because the island draws the avatar at 16 to 30 points, where its subtle drift is invisible.
struct Motion {
    face: Option<ExpressionId>,
    /// Breathing amplitude and period in seconds.
    breathe: f32,
    period: f32,
    /// Multiplier on the gaze drift.
    wander: f32,
    /// Head-tilt sway in degrees.
    sway: f32,
    /// Hop height in ball radii.
    bounce: f32,
    /// Vertical squash in ball radii.
    droop: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub body: [Pt; BODY_POINTS],
    /// Closed outlines; `None` when the eye faces away from the viewer.
    pub eyes: [Option<[Pt; EYE_POINTS]>; 2],
    /// The renderer strokes each eye as a smiling arc (the prototype's happy face) instead of filling it.
    pub smile: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Silhouette {
    Circle,
    Egg,
    Hexagon,
    Triangle,
}

/// A stable identity from a seed (agent name): silhouette, resting face and blink phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    pub silhouette: Silhouette,
    /// Index into the resting faces a seed may pick.
    pub resting: u8,
    pub phase: f32,
}

pub fn profile(seed: &str) -> Profile {
    let hash = fnv(seed);
    let silhouette = [
        Silhouette::Circle,
        Silhouette::Egg,
        Silhouette::Hexagon,
        Silhouette::Triangle,
    ][(hash % 4) as usize];
    Profile {
        silhouette,
        resting: ((hash >> 8) % RESTING.len() as u32) as u8,
        phase: ((hash >> 16) % 900) as f32 / 10.,
    }
}

fn fnv(seed: &str) -> u32 {
    seed.bytes().fold(0x811c9dc5u32, |h, b| {
        (h ^ b as u32).wrapping_mul(0x0100_0193)
    })
}

struct Tables {
    cos: [f32; BODY_POINTS],
    sin: [f32; BODY_POINTS],
    blinks: Vec<f32>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let angle = |i: usize| (i * 2) as f32 / PROFILE_SAMPLES as f32 * TAU;
        let mut rng = Mulberry(0x5eed);
        let (mut blinks, mut t) = (Vec::new(), 1.4f32);
        while t < BLINK_LOOP {
            blinks.push(t);
            t += 1.9 + rng.next() * 2.7;
            if rng.next() < 0.18 {
                blinks.push(t);
                t += 0.24;
            }
        }
        Tables {
            cos: std::array::from_fn(|i| angle(i).cos()),
            sin: std::array::from_fn(|i| angle(i).sin()),
            blinks,
        }
    })
}

/// Seconds after which the pre-drawn blink schedule repeats.
const BLINK_LOOP: f32 = 900.;
const BLINK_DUR: f32 = 0.18;

/// mulberry32, the generator Bloub uses for its blink schedule.
struct Mulberry(u32);

impl Mulberry {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = (self.0 ^ (self.0 >> 15)).wrapping_mul(1 | self.0);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64 / 4_294_967_296.0) as f32
    }
}

fn loop_noise(t: f32, period: f32, seed: f32) -> f32 {
    let p = t / period * TAU;
    0.55 * (p + seed).sin()
        + 0.3 * (2. * p + seed * 1.7 + 1.1).sin()
        + 0.15 * (3. * p + seed * 2.3 + 2.4).sin()
}

/// 1 = eye open, 0 = closed.
fn blink_lid(t: f32) -> f32 {
    let t = t.rem_euclid(BLINK_LOOP);
    for &start in &tables().blinks {
        if t < start {
            break;
        }
        let k = (t - start) / BLINK_DUR;
        if k <= 1. {
            return if k < 0.45 {
                1. - k / 0.45
            } else {
                (k - 0.45) / 0.55
            };
        }
    }
    1.
}

/// Scales that make the four filled silhouettes occupy the same area.
/// The prototype's 1 / 0.98 / 1.06 / 1.10 fit its SVG paths. These radial profiles
/// are smaller than the unit circle (the egg's raw area is about 0.81 of the circle),
/// so the same constants would leave the egg at 1.28× the circle. The factors below are
/// `sqrt(circle_area / profile_area)` on the 32-point body the renderer actually draws:
/// 1 / 1.114 / 1.055 / 1.153. Circle stays at 1, so a stroke cannot enlarge it.
pub fn silhouette_scale(silhouette: Silhouette) -> f32 {
    match silhouette {
        Silhouette::Circle => 1.,
        Silhouette::Egg => 1.114,
        Silhouette::Hexagon => 1.055,
        Silhouette::Triangle => 1.153,
    }
}

fn profile_radius(silhouette: Silhouette, index: usize) -> f32 {
    let raw = match silhouette {
        Silhouette::Circle => 1.,
        Silhouette::Egg => EGG[index % PROFILE_SAMPLES],
        Silhouette::Hexagon => HEXAGON[index % PROFILE_SAMPLES],
        Silhouette::Triangle => TRIANGLE[index % PROFILE_SAMPLES],
    };
    raw * silhouette_scale(silhouette)
}

fn radii(silhouette: Silhouette, sample: usize) -> f32 {
    profile_radius(silhouette, sample * 2)
}

fn radius_at(silhouette: Silhouette, angle: f32) -> f32 {
    let t = (angle / TAU).rem_euclid(1.) * PROFILE_SAMPLES as f32;
    let i = t.floor() as usize;
    let f = t - i as f32;
    profile_radius(silhouette, i) * (1. - f) + profile_radius(silhouette, i + 1) * f
}

/// Shoelace area of the resting 32-point body. Drift and breathing are not included.
pub fn resting_area(silhouette: Silhouette) -> f32 {
    let n = BODY_POINTS;
    let mut sum = 0.;
    for i in 0..n {
        let a0 = (i * 2) as f32 / PROFILE_SAMPLES as f32 * TAU;
        let a1 = ((i + 1) * 2) as f32 / PROFILE_SAMPLES as f32 * TAU;
        let r0 = radii(silhouette, i);
        let r1 = radii(silhouette, (i + 1) % n);
        let (x0, y0) = (r0 * a0.cos(), r0 * a0.sin());
        let (x1, y1) = (r1 * a1.cos(), r1 * a1.sin());
        sum += x0 * y1 - x1 * y0;
    }
    sum.abs() * 0.5
}

type V3 = [f32; 3];

fn spin(u: V3, v: V3, angle: f32) -> (V3, V3) {
    let (s, c) = angle.sin_cos();
    (
        [
            u[0] * c + v[0] * s,
            u[1] * c + v[1] * s,
            u[2] * c + v[2] * s,
        ],
        [
            v[0] * c - u[0] * s,
            v[1] * c - u[1] * s,
            v[2] * c - u[2] * s,
        ],
    )
}

/// Eye placement from a head orientation: (x, y, tangent a, b, c, d, depth) per eye.
fn eye_poses(yaw: f32, pitch: f32, roll: f32, split: f32) -> [[f32; 7]; 2] {
    let rad = f32::to_radians;
    let (f, right) = spin([0., 0., 1.], [1., 0., 0.], rad(yaw));
    let (down, f) = spin([0., 1., 0.], f, rad(pitch));
    let (right, down) = spin(right, down, rad(roll));
    let build = |side: f32| {
        let (ef, er) = spin(f, right, rad(split * side));
        [ef[0], ef[1], er[0], er[1], down[0], down[1], ef[2]]
    };
    [build(-1.), build(1.)]
}

fn capsule(w: f32, h: f32, m: [f32; 4], at: Pt) -> [Pt; EYE_POINTS] {
    let (hw, hh) = (w.max(0.01) / 2., h.max(0.01) / 2.);
    let r = hw.min(hh);
    // Top cap, then bottom cap; each cap is a half circle of 10 points.
    let (cy_top, cy_bottom) = (-hh + r, hh - r);
    std::array::from_fn(|i| {
        let half = EYE_POINTS / 2;
        let (k, cy, base) = if i < half {
            (i, cy_top, std::f32::consts::PI)
        } else {
            (i - half, cy_bottom, 0.)
        };
        let a = base + std::f32::consts::PI * k as f32 / (half - 1) as f32;
        let (px, py) = (a.cos() * r, cy + a.sin() * r);
        Pt {
            x: m[0] * px + m[2] * py + at.x,
            y: m[1] * px + m[3] * py + at.y,
        }
    })
}

/// The avatar at time `t` seconds. A pure function: the same inputs give the same frame.
pub fn sample(profile: &Profile, mood: Mood, t: f32) -> Frame {
    let tb = tables();
    let m = mood.motion();
    let resting = m
        .face
        .unwrap_or(RESTING[profile.resting as usize % RESTING.len()]);
    let exp: Expression = expression(resting);
    let t = t + profile.phase;

    let hop = if m.bounce > 0. {
        -(t / m.period * TAU).sin().abs() * m.bounce
    } else {
        0.
    };
    let drift = (
        loop_noise(t, 7.9, 1.9) * 0.006,
        loop_noise(t, 5.3, 0.3) * 0.007 + hop,
    );
    let scale = 1. - m.droop + m.breathe * (t / m.period * TAU).sin();

    let body = std::array::from_fn(|i| {
        let r = radii(profile.silhouette, i);
        Pt {
            x: r * tb.cos[i] + drift.0,
            y: r * tb.sin[i] * scale + drift.1,
        }
    });

    let dyaw = (loop_noise(t, 11.3, 0.4) * 5.5 + loop_noise(t, 3.7, 2.1) * 1.6) * m.wander;
    let dpitch = (loop_noise(t, 9.1, 1.3) * 4.2 + loop_noise(t, 4.3, 0.7) * 1.3) * m.wander;
    let droll = loop_noise(t, 13.7, 3.2) * 2.2 * m.wander + (t / 2.4 * TAU).sin() * m.sway;
    let lid = blink_lid(t);
    let poses = eye_poses(
        exp.yaw + dyaw,
        exp.pitch + dpitch,
        exp.roll + droll,
        exp.split,
    );

    let eyes = std::array::from_fn(|i| {
        let [x, y, a, b, c, d, depth] = poses[i];
        if depth <= 0.02 {
            return None;
        }
        let cfg = exp.eyes[i];
        let fit = radius_at(profile.silhouette, y.atan2(x));
        let face = silhouette_scale(profile.silhouette);
        let phi = cfg.tilt.to_radians();
        let (sp, cp) = phi.sin_cos();
        let k = 0.06 + 0.94 * lid.min(cfg.open).clamp(0., 1.);
        let m = [
            a * cp + c * sp,
            (b * cp + d * sp) * k,
            -a * sp + c * cp,
            (-b * sp + d * cp) * k,
        ];
        Some(capsule(
            cfg.w * face,
            cfg.h * face,
            m,
            Pt {
                x: x * fit + drift.0,
                y: y * fit + drift.1,
            },
        ))
    });
    Frame {
        body,
        eyes,
        smile: mood == Mood::Responded,
    }
}
