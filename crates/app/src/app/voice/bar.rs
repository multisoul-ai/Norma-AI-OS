//! Bottom voice bar rendering — Apple-style, state-driven.
//!
//! 这条悬浮在画布底部的语音条按 `VoiceVisual` 分类决定外观（图标、颜色、
//! 内容分支），`chrome.rs` 只负责把它放进画布的居中底部锚点。所有 8 个
//! `VoicePhase` 在 `control.rs` 里已被压缩成 5 类视觉，这里不再处理 phase
//! 细节，只翻译"应该长什么样"。

use super::control::VoiceVisual;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, MouseButton, Window};
use ui::Theme;

/// 底部语音条与画布底部的间距。
const VOICE_BAR_BOTTOM: f32 = 22.0;
/// Idle 状态下显示的快捷键键帽序列（Apple 菜单风格）。
const IDLE_KEYCAPS: &[&str] = &["⌘", "⇧", "V"];

impl MultisoulApp {
    /// 渲染底部语音条：一个水平居中、宽度自适应内容的圆角胶囊。
    /// 外层是全宽锚点（仅做水平居中），内层才是真正可见的 pill。
    pub(crate) fn render_voice_bar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let visual = self.voice.visual();

        // 外层全宽锚点，仅负责把 pill 水平居中并固定在底部。
        div()
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(VOICE_BAR_BOTTOM))
            .flex()
            .flex_row()
            .justify_center()
            .child(voice_pill(theme, visual, self, cx))
    }
}

/// 渲染可见的 pill 容器及其内部内容。按 `visual` 选择图标、状态色与内容分支。
fn voice_pill(
    theme: Theme,
    visual: VoiceVisual,
    app: &mut MultisoulApp,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let palette = voice_palette(theme, visual);

    div()
        .h(px(42.0))
        .min_w(px(168.0))
        .max_w(px(520.0))
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .px(px(16.0))
        .rounded(px(21.0))
        .border_1()
        .border_color(rgba(palette.border))
        .shadow(voice_shadow(theme))
        .bg(rgba(palette.fill))
        .text_color(rgba(palette.text))
        .text_size(px(13.0))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(MultisoulApp::on_voice_bar_mouse_down),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(MultisoulApp::on_voice_bar_mouse_up),
        )
        .child(status_glyph(theme, visual))
        .child(voice_content(theme, visual, app))
}

/// 按 `visual` 取色：图标色、文字色、pill 填充与描边。Idle 一律弱化。
fn voice_palette(theme: Theme, visual: VoiceVisual) -> VoicePalette {
    let idle_border = with_alpha(theme.edge_highlight, 0x22);
    let idle_fill = with_alpha(theme.panel, 0xc8);
    match visual {
        VoiceVisual::Idle => VoicePalette {
            icon: theme.text_tertiary,
            text: theme.text_tertiary,
            fill: idle_fill,
            border: idle_border,
        },
        VoiceVisual::Recording => VoicePalette {
            icon: theme.accent_orange,
            text: theme.text_primary,
            fill: with_alpha(theme.panel, 0xe5),
            border: with_alpha(theme.accent_orange, 0x55),
        },
        VoiceVisual::Processing => VoicePalette {
            icon: theme.accent_warm,
            text: theme.text_primary,
            fill: with_alpha(theme.panel, 0xe5),
            border: with_alpha(theme.accent_warm, 0x4d),
        },
        VoiceVisual::Success => VoicePalette {
            icon: theme.accent_green,
            text: theme.text_primary,
            fill: with_alpha(theme.panel, 0xe5),
            border: with_alpha(theme.accent_green, 0x4d),
        },
        VoiceVisual::Error => VoicePalette {
            icon: theme.destructive,
            text: theme.text_primary,
            fill: with_alpha(theme.panel, 0xe5),
            border: with_alpha(theme.destructive, 0x55),
        },
    }
}

/// 左侧状态图标：Idle/Recording 用麦克字符，Recording 叠加呼吸色点；
/// Processing 用 ◌，Success 用 ✓，Error 用 ✕。颜色取自 palette.icon。
fn status_glyph(theme: Theme, visual: VoiceVisual) -> Div {
    let palette = voice_palette(theme, visual);
    match visual {
        VoiceVisual::Idle => glyph_cell("🎙", palette.icon),
        VoiceVisual::Recording => div().flex().flex_row().items_center().gap_2().child(
            div()
                .w(px(8.0))
                .h(px(8.0))
                .rounded(px(4.0))
                .bg(rgba(theme.accent_orange)),
        ),
        VoiceVisual::Processing => glyph_cell("◌", palette.icon),
        VoiceVisual::Success => glyph_cell("✓", palette.icon),
        VoiceVisual::Error => glyph_cell("✕", palette.icon),
    }
}

/// 单字符图标的固定格子，保证图标与右侧内容垂直对齐。
fn glyph_cell(glyph: &'static str, color: u32) -> Div {
    div()
        .w(px(20.0))
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgba(color))
        .text_size(px(15.0))
        .child(glyph)
}

/// 静态"正在收音"波形：3 根高度不同的圆角竖条，accent_orange 配不同透明度，
/// 表达动态感但不驱动任何定时器。
fn static_waveform(theme: Theme) -> Div {
    let bars = [(6.0, 0x99u8), (12.0, 0xcc), (8.0, 0x99)];
    let mut row = div().flex().flex_row().items_center().gap_1().h(px(16.0));
    for (h, alpha) in bars {
        row = row.child(
            div()
                .w(px(3.0))
                .h(px(h))
                .rounded(px(1.5))
                .bg(rgba(with_alpha(theme.accent_orange, alpha))),
        );
    }
    row
}

/// pill 右侧的内容区，按 `visual` 分支：
/// Idle = 键帽提示；Recording = 波形 + 转写；其余 = 状态文字。
fn voice_content(theme: Theme, visual: VoiceVisual, app: &mut MultisoulApp) -> Div {
    match visual {
        VoiceVisual::Idle => div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .text_color(rgba(theme.text_tertiary))
            .child(div().text_size(px(12.0)).child("Voice"))
            .children(IDLE_KEYCAPS.iter().map(|k| voice_keycap(theme, k))),
        VoiceVisual::Recording => div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .min_w(px(120.0))
            .child(static_waveform(theme))
            .child(
                div()
                    .max_w(px(360.0))
                    .text_color(rgba(theme.text_primary))
                    .child(app.voice_label()),
            ),
        VoiceVisual::Processing | VoiceVisual::Success | VoiceVisual::Error => {
            div().max_w(px(420.0)).child(app.voice_label())
        }
    }
}

/// Idle 用的小键帽，风格参照 `settings/keyboard.rs` 的 `keycap()` 但更克制：
/// 更矮、更小圆角、半透明底、`text_tertiary` 描边与文字。
fn voice_keycap(theme: Theme, label: &'static str) -> Div {
    div()
        .h(px(20.0))
        .min_w(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .px(px(5.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x33)))
        .bg(rgba(with_alpha(theme.node_fill, 0x14)))
        .text_size(px(11.0))
        .text_color(rgba(theme.text_tertiary))
        .child(label)
}

/// 底部语音条的阴影——比顶部 toolbar 略轻，避免在画布底部过重。
fn voice_shadow(theme: Theme) -> Vec<gpui::BoxShadow> {
    vec![gpui::BoxShadow {
        color: rgba(theme.shadow_color).into(),
        offset: gpui::point(px(0.0), px(14.0)),
        blur_radius: px(28.0),
        spread_radius: px(-14.0),
    }]
}

struct VoicePalette {
    icon: u32,
    text: u32,
    fill: u32,
    border: u32,
}

/// `(color & 0xffffff00) | alpha`：把 0xRRGGBBAA 的 alpha 通道替换掉，
/// 与 `chrome.rs` / `settings/*` 里的同名本地约定保持一致。
fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
