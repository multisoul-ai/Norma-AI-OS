//! 风格输出的绘制图元：与 GPUI 解耦，便于在无窗口下断言几何与透明度。

use super::VoiceGlowPhase;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct GlowFrame {
    pub width: f32,
    pub height: f32,
    pub dock: Rect,
    pub energy: f32,
    pub intensity: f32,
    pub phase: VoiceGlowPhase,
    pub t_ms: f32,
    pub reduce_motion: bool,
    pub gain: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GlowPrim {
    /// 闭合多边形填充。
    Fill { pts: Vec<(f32, f32)>, rgba: u32 },
    /// 折线描边。
    Stroke {
        pts: Vec<(f32, f32)>,
        width: f32,
        rgba: u32,
    },
    /// 仅用模糊阴影出的柔光：GPUI 0.2.2 没有模糊滤镜，用 `paint_shadows` 近似。
    Glow {
        rect: Rect,
        radius: f32,
        blur: f32,
        rgba: u32,
    },
}

/// `rgb` 是 0xRRGGBB；`alpha` 钳到 0..=1。
pub(crate) fn color(rgb: u32, alpha: f32) -> u32 {
    (rgb << 8) | (alpha.clamp(0.0, 1.0) * 255.0).round() as u32
}

pub(crate) fn lerp(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount.clamp(0.0, 1.0)
}
