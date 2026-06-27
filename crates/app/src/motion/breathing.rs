//! Agent 卡片呼吸渐变边框的纯函数：色相循环 + 呼吸 alpha + 周期映射。
//!
//! 不依赖 GPUI 类型，只操作 `0xRRGGBBAA` 形式的 u32 颜色，方便在 GPUI 窗口外单测。

use crate::motion::util::EPSILON;
use std::time::Duration;
use ui::Theme;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreathingState {
    Inactive,
    Cool,
    Warm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BreathingIntent {
    Inactive,
    Cool,
    Warm,
    Done,
}

impl BreathingIntent {
    pub fn breathing_state(self) -> BreathingState {
        match self {
            Self::Inactive | Self::Done => BreathingState::Inactive,
            Self::Cool => BreathingState::Cool,
            Self::Warm => BreathingState::Warm,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Cool | Self::Warm)
    }
}

impl From<BreathingState> for BreathingIntent {
    fn from(state: BreathingState) -> Self {
        match state {
            BreathingState::Inactive => Self::Inactive,
            BreathingState::Cool => Self::Cool,
            BreathingState::Warm => Self::Warm,
        }
    }
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

/// 四角光晕固定色：还原 Uiverse 参考 CSS 的橙/蓝/粉/紫，不随主题或 Cool/Warm 状态切换。
pub const CORNER_GLOW_TOP_LEFT: u32 = 0xf8903aff;
pub const CORNER_GLOW_TOP_RIGHT: u32 = 0x00a6ffff;
pub const CORNER_GLOW_BOTTOM_LEFT: u32 = 0xff0056ff;
pub const CORNER_GLOW_BOTTOM_RIGHT: u32 = 0x6500ffff;

const CORNER_GLOW_BLUR_MIN: f32 = 8.0;
const CORNER_GLOW_BLUR_MAX: f32 = 14.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerGlow {
    pub blur: f32,
    pub alpha: f32,
}

/// 呼吸包络：phase∈[0,1) → 0.0..=1.0，与 breathing_alpha 共享同一个 sin 公式，
/// 但映射到 0..1 而不是 0x66..0xFF 的字节区间，供 blur 与 alpha 共同复用。
pub fn corner_glow_envelope(phase: f32) -> f32 {
    let s = (phase * std::f32::consts::TAU).sin();
    (s + 1.0) / 2.0
}

/// 给定相位、呼吸淡入淡出强度与当前呼吸态，解析出四角光晕的模糊/强度参数。
/// Inactive 或已完全淡出（fade_value <= EPSILON）时返回 None，渲染层应据此完全跳过四角光晕。
pub fn resolve_corner_glow(
    phase: f32,
    fade_value: f32,
    state: BreathingState,
) -> Option<CornerGlow> {
    if state == BreathingState::Inactive || fade_value <= EPSILON {
        return None;
    }
    let envelope = corner_glow_envelope(phase);
    Some(CornerGlow {
        blur: CORNER_GLOW_BLUR_MIN + envelope * (CORNER_GLOW_BLUR_MAX - CORNER_GLOW_BLUR_MIN),
        alpha: fade_value * envelope,
    })
}

/// Chase 追尾光的渲染输出参数，传给 chrome 层使用。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChaseGlow {
    /// 追尾头在边框周长上的位置，0..1 顺时针（TL=0, TR=0.25, BR=0.5, BL=0.75）
    pub phase: f32,
    /// 整体淡入淡出强度 0..1
    pub fade: f32,
}

/// 给定当前追尾相位与某角的固定相位，返回该角的发光强度 0..1。
/// 追尾头最亮（behind=0），向后拖尾衰减至 TAIL_LENGTH 处归零。
pub fn chase_corner_intensity(chase_phase: f32, corner_phase: f32) -> f32 {
    let behind = (chase_phase - corner_phase + 1.0).rem_euclid(1.0);
    const TAIL_LENGTH: f32 = 0.35;
    if behind < TAIL_LENGTH {
        (1.0 - behind / TAIL_LENGTH).powf(1.5)
    } else {
        0.0
    }
}

#[cfg(test)]
#[path = "../../tests/unit/motion_breathing_tests.rs"]
mod tests;
