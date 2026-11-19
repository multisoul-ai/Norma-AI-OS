//! 声纹：Dock 上方浮起三条彩色声波丝带，中间饱满、两端收细（Siri 声波式）。
//! 聆听是一条细线；思考是一个来回移动的小波包；说话是三色丝带，振幅跟随能量。

use crate::effects::voice_glow::scene::{color, GlowFrame, GlowPrim};
use crate::effects::voice_glow::VoiceGlowPhase;

const POINTS: usize = 96;
/// (颜色, 空间频率, 相位速度 rad/ms, 初相, 振幅系数)
const WAVES: [(u32, f32, f32, f32, f32); 3] = [
    (0xFFB3D9, 1.6, 0.0042, 0.0, 1.0),
    (0xC5B5FF, 2.3, -0.0031, 2.0, 0.85),
    (0x9FD8FF, 3.1, 0.0055, 4.0, 0.7),
];
const ICE: u32 = 0x9FD8FF;
const LAVENDER: u32 = 0xC5B5FF;

fn envelope(u: f32) -> f32 {
    if u.abs() >= 1.0 {
        0.0
    } else {
        (1.0 - u * u).powf(2.2)
    }
}

pub(crate) fn build(frame: &GlowFrame) -> Vec<GlowPrim> {
    let intensity = frame.intensity.clamp(0.0, 1.0);
    if intensity < 0.004 {
        return Vec::new();
    }
    let energy = frame.energy.clamp(0.0, 1.0);
    let cx = frame.dock.x + frame.dock.w / 2.0;
    let base = frame.dock.y - 40.0;
    let half = (frame.width * 0.42).min(frame.dock.w * 0.95);
    let time = if frame.reduce_motion { 0.0 } else { frame.t_ms };
    let alpha = intensity * frame.gain;
    let xs = |i: usize| cx - half + 2.0 * half * i as f32 / POINTS as f32;
    let u = |x: f32| (x - cx) / half;

    match frame.phase {
        VoiceGlowPhase::Listening | VoiceGlowPhase::Thinking => {
            let listening = frame.phase == VoiceGlowPhase::Listening;
            let (amp, rgb) = if listening {
                (6.0 + 42.0 * energy, ICE)
            } else {
                (14.0, LAVENDER)
            };
            let bump = if frame.reduce_motion {
                0.0
            } else {
                (frame.t_ms * 0.0022).sin()
            };
            let pts = (0..=POINTS)
                .map(|i| {
                    let x = xs(i);
                    let shape = if listening {
                        envelope(u(x))
                    } else {
                        (-(((u(x) - bump * 0.6) / 0.18).powi(2))).exp()
                    };
                    let wave = (2.4 * std::f32::consts::PI * u(x) + time * 0.006).sin();
                    (x, base - amp * shape * wave)
                })
                .collect::<Vec<_>>();
            vec![
                GlowPrim::Stroke {
                    pts: pts.clone(),
                    width: 4.0,
                    rgba: color(rgb, 0.35 * alpha),
                },
                GlowPrim::Stroke {
                    pts,
                    width: 1.4,
                    rgba: color(rgb, alpha),
                },
            ]
        }
        _ => {
            let mut prims = Vec::with_capacity(WAVES.len() * 2 + 1);
            for (rgb, freq, speed, phase0, weight) in WAVES {
                let amp = (14.0 + 70.0 * energy) * weight * intensity;
                let phase = phase0 + time * speed;
                // 先画放大 12%、低透明度的发光层，再画本体。
                for (scale, a) in [(1.12, 0.25), (1.0, 0.7)] {
                    prims.push(GlowPrim::Fill {
                        pts: ribbon_polygon(base, amp * scale, freq, phase, &xs, &u),
                        rgba: color(rgb, a * alpha),
                    });
                }
            }
            let line = (0..=POINTS)
                .map(|i| {
                    let x = xs(i);
                    let wave = (2.0 * std::f32::consts::PI * u(x) + time * 0.004).sin();
                    (
                        x,
                        base - (4.0 + 30.0 * energy) * intensity * envelope(u(x)) * wave,
                    )
                })
                .collect();
            prims.push(GlowPrim::Stroke {
                pts: line,
                width: 1.0,
                rgba: color(0xF4EEFF, 0.9 * alpha),
            });
            prims
        }
    }
}

/// 上缘为完整振幅、下缘为 35% 振幅的闭合带：先上缘从左到右，再下缘从右到左。
fn ribbon_polygon(
    base: f32,
    amp: f32,
    freq: f32,
    phase: f32,
    xs: &impl Fn(usize) -> f32,
    u: &impl Fn(f32) -> f32,
) -> Vec<(f32, f32)> {
    let wave = |x: f32| envelope(u(x)) * (freq * std::f32::consts::PI * u(x) + phase).sin();
    let upper = (0..=POINTS).map(|i| (xs(i), base - amp * wave(xs(i))));
    let lower = (0..=POINTS)
        .rev()
        .map(|i| (xs(i), base + amp * 0.35 * wave(xs(i))));
    upper.chain(lower).collect()
}
