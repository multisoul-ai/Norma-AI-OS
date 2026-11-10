//! 光环：整扇窗口被一圈流动的彩色光包住（Apple Intelligence 式边缘光）。
//! 外层大柔光 + 中层柔光 + 内层 2px 亮边；厚度跟随能量，色环缓慢流动。

use crate::effects::voice_glow::scene::{color, lerp, GlowFrame, GlowPrim, Rect};
use crate::effects::voice_glow::VoiceGlowPhase;

const RING: [u32; 6] = [0xBC82F3, 0xF5B9EA, 0x8D9FFF, 0xFF6778, 0xFFBA71, 0xC686FF];
/// 每条边切成的段数；每段一个模糊阴影，段间重叠 30% 以消除色阶。
const SEGMENTS: usize = 12;
const FLOW_PER_MS: f32 = 0.00007;

fn ring_rgb(position: f32) -> u32 {
    let scaled = position.rem_euclid(1.0) * RING.len() as f32;
    let index = scaled as usize % RING.len();
    let (from, to) = (RING[index], RING[(index + 1) % RING.len()]);
    let mix = scaled.fract();
    let channel = |shift: u32| {
        lerp(
            ((from >> shift) & 0xff) as f32,
            ((to >> shift) & 0xff) as f32,
            mix,
        )
        .round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

pub(crate) fn build(frame: &GlowFrame) -> Vec<GlowPrim> {
    let intensity = frame.intensity.clamp(0.0, 1.0);
    if intensity < 0.004 {
        return Vec::new();
    }
    let energy = frame.energy.clamp(0.0, 1.0);
    let breath = 0.5 + 0.5 * (frame.t_ms * 0.0039).sin();
    let thickness = match frame.phase {
        VoiceGlowPhase::Listening => 0.2 + 0.6 * energy,
        VoiceGlowPhase::Thinking => 0.3 + 0.25 * breath,
        _ => 0.45 + 0.55 * energy,
    };
    let flow = if frame.reduce_motion {
        0.0
    } else {
        frame.t_ms * FLOW_PER_MS
    };
    let alpha = 0.9 * intensity * frame.gain;
    let (w, h) = (frame.width, frame.height);
    let mut prims = Vec::with_capacity(2 * 4 * SEGMENTS + 1);
    for blur in [22.0 + 48.0 * thickness, 7.0 + 14.0 * thickness] {
        for edge in 0..4 {
            for i in 0..SEGMENTS {
                let along = (i as f32 + 0.5) / SEGMENTS as f32;
                let position = (edge as f32 + along) / 4.0 + flow;
                let rgba = color(ring_rgb(position), alpha);
                let rect = segment(edge, i, w, h, blur);
                prims.push(GlowPrim::Glow {
                    rect,
                    radius: 0.0,
                    blur,
                    rgba,
                });
            }
        }
    }
    prims.push(GlowPrim::Stroke {
        pts: vec![
            (1.0, 1.0),
            (w - 1.0, 1.0),
            (w - 1.0, h - 1.0),
            (1.0, h - 1.0),
            (1.0, 1.0),
        ],
        width: 2.0,
        rgba: color(
            0xFFFFFF,
            0.55 * intensity * frame.gain * (0.4 + 0.6 * thickness),
        ),
    });
    prims
}

/// 贴着窗口外沿的细条：阴影向内渗入，窗口外的部分被内容遮罩裁掉。
fn segment(edge: usize, index: usize, w: f32, h: f32, blur: f32) -> Rect {
    let (length, depth) = if edge % 2 == 0 { (w, h) } else { (h, w) };
    let step = length / SEGMENTS as f32;
    let start = index as f32 * step - step * 0.15;
    let span = step * 1.3;
    let _ = depth;
    match edge {
        0 => Rect {
            x: start,
            y: -blur,
            w: span,
            h: blur,
        },
        1 => Rect {
            x: w,
            y: start,
            w: blur,
            h: span,
        },
        2 => Rect {
            x: start,
            y: h,
            w: span,
            h: blur,
        },
        _ => Rect {
            x: -blur,
            y: start,
            w: blur,
            h: span,
        },
    }
}
