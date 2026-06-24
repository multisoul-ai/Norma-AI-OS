//! `motion.rs` 的纯函数辅助：颜色插值、easing、rect 插值、视觉优先级判定。
//!
//! 拆出本文件只是为了让 `motion.rs` 保持在单文件行数上限以内，逻辑仍属于 motion 子系统。

use canvas::{CanvasRect, Vec2};
use std::ffi::OsStr;

use crate::motion::VisualPriority;

pub(crate) const EPSILON: f32 = 0.0001;

pub(crate) fn visual_priority(
    hover: f32,
    pressed: f32,
    dragging: f32,
    selected: f32,
    focused: f32,
) -> VisualPriority {
    if focused > EPSILON {
        VisualPriority::Focused
    } else if selected > EPSILON {
        VisualPriority::Selected
    } else if dragging > EPSILON {
        VisualPriority::Dragging
    } else if pressed > EPSILON {
        VisualPriority::Pressed
    } else if hover > EPSILON {
        VisualPriority::Hover
    } else {
        VisualPriority::Idle
    }
}

pub(crate) fn duration_progress(
    elapsed: std::time::Duration,
    duration: std::time::Duration,
) -> f32 {
    if duration.is_zero() {
        1.0
    } else {
        (elapsed.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
    }
}

pub(crate) fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

pub(crate) fn lerp_rect(a: CanvasRect, b: CanvasRect, t: f32) -> CanvasRect {
    CanvasRect {
        origin: Vec2::new(
            lerp(a.origin.x, b.origin.x, t),
            lerp(a.origin.y, b.origin.y, t),
        ),
        size: Vec2::new(lerp(a.size.x, b.size.x, t), lerp(a.size.y, b.size.y, t)),
    }
}

pub(crate) fn approx_rect(a: CanvasRect, b: CanvasRect) -> bool {
    (a.origin.x - b.origin.x).abs() <= EPSILON
        && (a.origin.y - b.origin.y).abs() <= EPSILON
        && (a.size.x - b.size.x).abs() <= EPSILON
        && (a.size.y - b.size.y).abs() <= EPSILON
}

pub(crate) fn mix_color(from: u32, to: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let fr = ((from >> 24) & 0xff) as f32;
    let fg = ((from >> 16) & 0xff) as f32;
    let fb = ((from >> 8) & 0xff) as f32;
    let fa = (from & 0xff) as f32;
    let tr = ((to >> 24) & 0xff) as f32;
    let tg = ((to >> 16) & 0xff) as f32;
    let tb = ((to >> 8) & 0xff) as f32;
    let ta = (to & 0xff) as f32;

    ((lerp(fr, tr, t).round() as u32) << 24)
        | ((lerp(fg, tg, t).round() as u32) << 16)
        | ((lerp(fb, tb, t).round() as u32) << 8)
        | (lerp(fa, ta, t).round() as u32)
}

pub(crate) fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

pub(crate) fn alpha(color: u32) -> u8 {
    (color & 0xff) as u8
}

pub(crate) fn reduce_motion_from_env_value(value: Option<impl AsRef<OsStr>>) -> bool {
    value
        .and_then(|value| value.as_ref().to_str().map(str::to_owned))
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}
