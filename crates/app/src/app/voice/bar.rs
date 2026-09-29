//! Bottom voice bar rendering — Apple-style, state-driven.
//!
//! 这条悬浮在画布底部的语音条按 `VoiceVisual` 分类决定外观（图标、颜色、
//! 内容分支），`chrome.rs` 只负责把它放进画布的居中底部锚点。所有 8 个
//! `VoicePhase` 在 `control.rs` 里已被压缩成 5 类视觉，这里不再处理 phase
//! 细节，只翻译"应该长什么样"。

use super::control::VoiceVisual;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, svg, Context, Div, SharedString, Stateful, Window};
use ui::Theme;

impl MultisoulApp {
    /// 渲染 Command Dock 内的语音状态单元，保留原状态机与恢复入口。
    pub(crate) fn render_voice_bar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        let visual = self.effective_voice_visual();

        voice_pill(theme, visual, self, cx)
    }
}

/// 渲染可见的 pill 容器及其内部内容。按 `visual` 选择图标、状态色与内容分支。
fn voice_pill(
    theme: Theme,
    visual: VoiceVisual,
    app: &mut MultisoulApp,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let palette = voice_palette(theme, visual);
    div()
        .id("workspace-voice-control")
        .debug_selector(|| "workspace-voice-control".to_string())
        .h(px(38.0))
        .min_w(px(88.0))
        .max_w(px(520.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .px(px(13.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(rgba(palette.border))
        .bg(rgba(palette.fill))
        .text_color(rgba(palette.text))
        .text_size(px(11.0))
        .cursor_pointer()
        .on_click(cx.listener(MultisoulApp::on_voice_bar_click))
        .child(interactive_status_glyph(theme, visual, cx))
        .child(voice_content(theme, visual, app, cx))
}

fn interactive_status_glyph(
    theme: Theme,
    visual: VoiceVisual,
    cx: &mut Context<MultisoulApp>,
) -> gpui::AnyElement {
    if visual != VoiceVisual::Error {
        return status_glyph(theme, visual).into_any_element();
    }

    let palette = voice_palette(theme, visual);
    div()
        .id("voice-error-dismiss")
        .debug_selector(|| "voice-error-dismiss".to_string())
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .text_color(rgba(palette.icon))
        .text_size(px(15.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(with_alpha(theme.destructive, 0x24))))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|app, _, _window, cx| {
            cx.stop_propagation();
            app.dismiss_voice_error();
            cx.notify();
        }))
        .child("✕")
        .into_any_element()
}

/// 按 `visual` 取色：图标色、文字色、pill 填充与描边。Idle 一律弱化。
fn voice_palette(theme: Theme, visual: VoiceVisual) -> VoicePalette {
    match visual {
        VoiceVisual::Idle => VoicePalette {
            icon: 0xffffffd6,
            text: 0xffffffff,
            fill: 0xffffff00,
            border: 0xffffff00,
        },
        VoiceVisual::Recording => VoicePalette {
            icon: theme.accent_orange,
            text: 0xffffffff,
            fill: 0xffffff12,
            border: with_alpha(theme.accent_orange, 0x55),
        },
        VoiceVisual::Processing => VoicePalette {
            icon: theme.accent_warm,
            text: 0xffffffff,
            fill: 0xffffff12,
            border: with_alpha(theme.accent_warm, 0x4d),
        },
        VoiceVisual::Success => VoicePalette {
            icon: theme.accent_green,
            text: 0xffffffff,
            fill: 0xffffff12,
            border: with_alpha(theme.accent_green, 0x4d),
        },
        VoiceVisual::Error => VoicePalette {
            icon: theme.destructive,
            text: 0xffffffff,
            fill: 0xffffff12,
            border: with_alpha(theme.destructive, 0x55),
        },
    }
}

/// 左侧状态图标：Idle/Recording 用麦克字符，Recording 叠加呼吸色点；
/// Processing 用 ◌，Success 用 ✓，Error 用 ✕。颜色取自 palette.icon。
fn status_glyph(theme: Theme, visual: VoiceVisual) -> Div {
    let palette = voice_palette(theme, visual);
    match visual {
        VoiceVisual::Idle => voice_icon_cell(palette.icon),
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

fn voice_icon_cell(color: u32) -> Div {
    div()
        .w(px(16.0))
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            svg()
                .path(SharedString::from("icons/settings-nav-voice.svg"))
                .text_color(rgba(color))
                .w(px(16.0))
                .h(px(16.0)),
        )
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
fn voice_content(
    theme: Theme,
    visual: VoiceVisual,
    app: &mut MultisoulApp,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    match visual {
        VoiceVisual::Idle => {
            let shortcut = app.shortcuts.voice_interaction_label();
            let label = match app.voice_settings.readiness() {
                crate::settings::voice::VoiceEntryState::SetUp { .. } => {
                    format!("Set up Voice  {shortcut}")
                }
                crate::settings::voice::VoiceEntryState::SettingUp { percent } => {
                    format!("Setting up Voice · {percent}%")
                }
                crate::settings::voice::VoiceEntryState::Ready => {
                    if app.voice_settings.values().desktop_mode()
                        == crate::settings::voice::DesktopVoiceMode::Doubao
                    {
                        format!("Doubao 2.0 Voice  {shortcut}")
                    } else {
                        format!("Voice  {shortcut}")
                    }
                }
                crate::settings::voice::VoiceEntryState::Unavailable { .. } => {
                    "Voice unavailable".to_string()
                }
            };
            div()
                .flex()
                .items_center()
                .text_color(rgba(0xffffffff))
                .child(label)
        }
        VoiceVisual::Recording => div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(static_waveform(theme))
            .child(
                div()
                    .whitespace_nowrap()
                    .text_color(rgba(0xffffffff))
                    .child(app.effective_voice_dock_label()),
            )
            .child(
                div()
                    .id("workspace-voice-shortcut")
                    .debug_selector(|| "workspace-voice-shortcut".to_string())
                    .whitespace_nowrap()
                    .text_color(rgba(0xffffff8a))
                    .child(app.shortcuts.voice_interaction_label()),
            ),
        VoiceVisual::Processing | VoiceVisual::Success => div()
            .whitespace_nowrap()
            .child(app.effective_voice_dock_label()),
        VoiceVisual::Error => {
            let mut content = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .max_w(px(460.0))
                .child(app.effective_voice_label());
            if app.can_open_microphone_privacy_settings() {
                content = content.child(microphone_settings_button(theme, cx));
            }
            content
        }
    }
}

impl MultisoulApp {
    fn on_voice_bar_click(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.voice_settings.readiness(),
            crate::settings::voice::VoiceEntryState::Ready
        ) {
            self.activate_ready_voice(true, window, cx);
        } else {
            self.open_voice_settings(window, cx);
        }
    }

    pub(crate) fn render_voice_ready_notification(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        div()
            .id("voice-ready-notification")
            .debug_selector(|| "voice-ready-notification".to_string())
            .absolute()
            .top(px(22.0))
            .right(px(22.0))
            .max_w(px(320.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .p(px(13.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgba(with_alpha(theme.accent_green, 0x70)))
            .bg(rgba(with_alpha(theme.panel, 0xf2)))
            .text_color(rgba(theme.text_primary))
            .child(glyph_cell("✓", theme.accent_green))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child("Voice is ready")
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(theme.text_secondary))
                            .child(format!(
                                "Press {} to start {}.",
                                self.shortcuts.voice_interaction_label(),
                                if self.voice_settings.values().desktop_mode()
                                    == crate::settings::voice::DesktopVoiceMode::Doubao
                                {
                                    "a Doubao call"
                                } else {
                                    "a voice command"
                                }
                            )),
                    ),
            )
            .child(
                div()
                    .id("voice-ready-notification-close")
                    .cursor_pointer()
                    .text_color(rgba(theme.text_secondary))
                    .child("×")
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.voice_ready_notification_visible = false;
                        cx.notify();
                    })),
            )
    }
}

fn microphone_settings_button(theme: Theme, cx: &mut Context<MultisoulApp>) -> impl IntoElement {
    div()
        .id("voice-open-microphone-settings")
        .debug_selector(|| "voice-open-microphone-settings".to_string())
        .h(px(26.0))
        .px(px(9.0))
        .flex()
        .items_center()
        .rounded(px(7.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.destructive, 0x70)))
        .bg(rgba(with_alpha(theme.destructive, 0x18)))
        .text_size(px(12.0))
        .text_color(rgba(0xffffffff))
        .cursor_pointer()
        .on_click(cx.listener(MultisoulApp::on_voice_open_microphone_privacy_settings))
        .child("Open Microphone Settings")
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
