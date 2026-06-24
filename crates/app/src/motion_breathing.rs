//! Agent 卡片呼吸渐变边框的纯函数：色相循环 + 呼吸 alpha + 周期映射。
//!
//! 不依赖 GPUI 类型，只操作 `0xRRGGBBAA` 形式的 u32 颜色，方便在 GPUI 窗口外单测。

use std::time::Duration;
use ui::Theme;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreathingState {
    Inactive,
    Cool,
    Warm,
}

impl BreathingState {
    pub fn period(self) -> Duration {
        match self {
            BreathingState::Inactive => Duration::ZERO,
            BreathingState::Cool => Duration::from_millis(2500),
            BreathingState::Warm => Duration::from_millis(1200),
        }
    }
}

/// 在 phase∈[0,1) 循环内，沿 colors 做分段 mix，构成连续色相循环。
fn breathing_hue_color(phase: f32, colors: &[u32]) -> u32 {
    let n = colors.len();
    debug_assert!(n >= 2, "breathing color set 至少需要两个颜色才能循环插值");
    let phase = phase.rem_euclid(1.0);
    let seg = 1.0 / n as f32;
    let idx = ((phase / seg).floor() as usize).min(n - 1);
    let local_t = (phase - idx as f32 * seg) / seg;
    let next = (idx + 1) % n;
    mix_color(colors[idx], colors[next], local_t)
}

/// 呼吸亮度：sin(phase*2π) 把 alpha 映射到 0x66~0xFF 区间，形成亮—暗—亮起伏。
fn breathing_alpha(phase: f32) -> u8 {
    let s = (phase * std::f32::consts::TAU).sin();
    let t = (s + 1.0) / 2.0;
    let low = 0x66 as f32;
    let high = 0xFF as f32;
    (low + t * (high - low)).round() as u8
}

/// 给定相位与色组，返回当前帧的呼吸边框颜色（色相循环 + alpha 呼吸叠加）。
pub fn breathing_color_for_phase(phase: f32, state: BreathingState, theme: Theme) -> u32 {
    let hue = match state {
        BreathingState::Inactive => return theme.node_stroke,
        BreathingState::Cool => breathing_hue_color(
            phase,
            &[theme.accent_cyan, theme.accent_violet, theme.accent_blue],
        ),
        BreathingState::Warm => {
            breathing_hue_color(phase, &[theme.accent_warm, theme.accent_orange])
        }
    };
    with_alpha(hue, breathing_alpha(phase))
}

fn mix_color(from: u32, to: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let fr = ((from >> 24) & 0xff) as f32;
    let fg = ((from >> 16) & 0xff) as f32;
    let fb = ((from >> 8) & 0xff) as f32;
    let fa = (from & 0xff) as f32;
    let tr = ((to >> 24) & 0xff) as f32;
    let tg = ((to >> 16) & 0xff) as f32;
    let tb = ((to >> 8) & 0xff) as f32;
    let ta = (to & 0xff) as f32;

    (((fr + (tr - fr) * t).round() as u32) << 24)
        | (((fg + (tg - fg) * t).round() as u32) << 16)
        | (((fb + (tb - fb) * t).round() as u32) << 8)
        | ((fa + (ta - fa) * t).round() as u32)
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
#[path = "../tests/unit/motion_breathing_tests.rs"]
mod tests;
