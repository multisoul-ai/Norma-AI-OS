//! 运行特效的语义色：取自主题 token，深浅主题各自成立。
//! 蓝 = 思考、青 = 工具、暖色 = 等你、绿 = 完成、红 = 失败。

use super::RunState;
use crate::motion::util::mix_color;
use ui::Theme;

/// 0..255 的 RGB 浮点，便于逐帧平滑。
pub(crate) type Rgb = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RunPalette {
    ready: u32,
    thinking: u32,
    tool: u32,
    approval: u32,
    done: u32,
    failed: u32,
    /// 灵光的四色光谱，按状态。
    spectra: [[u32; 4]; 6],
}

impl RunPalette {
    pub(crate) fn from_theme(theme: Theme) -> Self {
        let mix = |a: u32, b: u32, t: f32| mix_color(a, b, t);
        let rose = mix(theme.accent_violet, theme.destructive, 0.55);
        let ready = theme.text_tertiary;
        Self {
            ready,
            thinking: theme.accent_blue,
            tool: theme.accent_cyan,
            approval: theme.accent_warm,
            done: theme.accent_green,
            failed: theme.destructive,
            spectra: [
                [ready; 4],
                [
                    theme.accent_blue,
                    theme.accent_violet,
                    rose,
                    theme.accent_cyan,
                ],
                [
                    theme.accent_cyan,
                    theme.accent_blue,
                    theme.accent_violet,
                    mix(theme.accent_cyan, theme.accent_green, 0.5),
                ],
                [
                    theme.accent_warm,
                    theme.accent_orange,
                    mix(theme.accent_warm, theme.accent_orange, 0.5),
                    mix(theme.accent_orange, theme.destructive, 0.4),
                ],
                [
                    theme.accent_green,
                    mix(theme.accent_green, theme.accent_cyan, 0.5),
                    theme.accent_green,
                    mix(theme.accent_green, theme.accent_warm, 0.3),
                ],
                [
                    theme.destructive,
                    mix(theme.destructive, theme.graphite, 0.45),
                    theme.destructive,
                    mix(theme.destructive, theme.accent_orange, 0.3),
                ],
            ],
        }
    }

    pub(crate) fn state_color(&self, state: RunState) -> Rgb {
        rgb(match state {
            RunState::Ready => self.ready,
            RunState::Thinking => self.thinking,
            RunState::Tool => self.tool,
            RunState::Approval => self.approval,
            RunState::Done => self.done,
            RunState::Failed => self.failed,
        })
    }

    pub(crate) fn spectrum(&self, state: RunState) -> [Rgb; 4] {
        self.spectra[state.index()].map(rgb)
    }
}

pub(crate) fn rgb(color: u32) -> Rgb {
    [
        ((color >> 24) & 0xff) as f32,
        ((color >> 16) & 0xff) as f32,
        ((color >> 8) & 0xff) as f32,
    ]
}

/// RGB + 0..1 透明度 → `0xRRGGBBAA`。
pub(crate) fn with_alpha_f(color: Rgb, alpha: f32) -> u32 {
    let channel = |v: f32| v.round().clamp(0.0, 255.0) as u32;
    (channel(color[0]) << 24)
        | (channel(color[1]) << 16)
        | (channel(color[2]) << 8)
        | channel(alpha.clamp(0.0, 1.0) * 255.0)
}

pub(crate) fn mix_rgb(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// 在四色光谱上按 `x`（取模 1）做平滑插值。
pub(crate) fn spectrum_at(spectrum: &[Rgb; 4], x: f32) -> Rgb {
    let s = x.rem_euclid(1.0) * 4.0;
    let i = (s.floor() as usize).min(3);
    let f = s - i as f32;
    mix_rgb(spectrum[i], spectrum[(i + 1) % 4], f * f * (3.0 - 2.0 * f))
}
