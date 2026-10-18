use crate::settings::spacing::CARD_PADDING_COMPACT;
use crate::settings::typography::{SettingsText, TextRole};
use gpui::{
    div, prelude::*, px, rgba, AnyElement, App, ClickEvent, Div, FontWeight, KeyDownEvent, Window,
};
use std::rc::Rc;
use ui::Theme;

pub(super) fn action_button(
    theme: Theme,
    id: &'static str,
    label: &'static str,
    accent: u32,
    on_activate: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let on_activate = Rc::new(on_activate);
    let on_click = on_activate.clone();
    let on_key_down = on_activate.clone();
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(accent, 0x70)))
        .bg(rgba(with_alpha(accent, 0x18)))
        .settings_font(TextRole::Control)
        .text_color(rgba(accent))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(accent, 0x28))))
        .active(move |style| style.bg(rgba(with_alpha(accent, 0x38))))
        .tab_index(0)
        .focus(move |style| {
            style
                .border_color(rgba(accent))
                .bg(rgba(with_alpha(theme.panel_high, 0xd8)))
        })
        .on_click(move |event, window, cx| on_click(event, window, cx))
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                on_key_down(&ClickEvent::default(), window, cx);
                cx.stop_propagation();
            }
        })
        .child(label)
        .into_any_element()
}

pub(super) fn disabled_action_button(
    theme: Theme,
    id: &'static str,
    label: &'static str,
) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x38)))
        .bg(rgba(with_alpha(theme.panel_high, 0x38)))
        .settings_font(TextRole::Control)
        .text_color(rgba(with_alpha(theme.text_secondary, 0x70)))
        .child(label)
        .into_any_element()
}

pub(super) fn status_block(theme: Theme, message: &str, is_error: bool) -> Div {
    div()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(
            if is_error {
                theme.destructive
            } else {
                theme.accent_cyan
            },
            0x40,
        )))
        .bg(rgba(with_alpha(
            if is_error {
                theme.destructive
            } else {
                theme.panel_high
            },
            0x16,
        )))
        .px(px(CARD_PADDING_COMPACT))
        .py(px(CARD_PADDING_COMPACT))
        .settings_font(TextRole::Body)
        .text_color(rgba(if is_error {
            theme.destructive
        } else {
            theme.text_secondary
        }))
        .child(message.to_string())
}

pub(super) fn empty_list_message(theme: Theme, message: &'static str) -> Div {
    div()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .bg(rgba(with_alpha(theme.panel_high, 0x60)))
        .px(px(CARD_PADDING_COMPACT))
        .py(px(CARD_PADDING_COMPACT))
        .settings_text(TextRole::Helper, theme)
        .child(message)
}

pub(super) fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

/// 将 `color` 的 RGB 向 `target` 混合 `amount`（0.0–1.0），alpha 保持不变。
pub(super) fn mix_rgb(color: u32, target: u32, amount: f32) -> u32 {
    let blend = |shift: u32| {
        let from = ((color >> shift) & 0xff) as f32;
        let to = ((target >> shift) & 0xff) as f32;
        (from + (to - from) * amount).round().clamp(0.0, 255.0) as u32
    };
    (blend(24) << 24) | (blend(16) << 16) | (blend(8) << 8) | (color & 0xff)
}

/// Runtime 头像底色；固定色相保证不同 Runtime 在所有主题下都能被一眼区分。
pub(super) fn profile_avatar_color(profile: canvas::AgentProfile) -> u32 {
    match profile {
        canvas::AgentProfile::Claude => 0xd97757ff,
        canvas::AgentProfile::Codex => 0x2f80edff,
        canvas::AgentProfile::Cursor => 0x4a5560ff,
        canvas::AgentProfile::Grok => 0x6a5cffff,
        canvas::AgentProfile::OpenCode => 0x2fa57fff,
        // 占位色：互相区分即可，品牌色待各 CLI 负责人取得官方素材后调整。
        canvas::AgentProfile::Kimi => 0x1f6febff,
        canvas::AgentProfile::ZCode => 0x0f9d8aff,
        canvas::AgentProfile::Pi => 0xb5651dff,
        canvas::AgentProfile::Hermes => 0xc2415dff,
    }
}

pub(super) fn avatar(name: &str, profile: canvas::AgentProfile, size: f32) -> Div {
    div()
        .w(px(size))
        .h(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(size / 2.0))
        .bg(rgba(profile_avatar_color(profile)))
        .text_size(px((size * 0.42).round()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba(0xffffffff))
        .child(name.chars().next().unwrap_or('A').to_string())
}

/// 主操作按钮：实心强调色，hover 提亮、pressed 压暗、键盘焦点显示深色描边。
pub(super) fn primary_button(
    theme: Theme,
    id: &'static str,
    label: &'static str,
    compact: bool,
    on_activate: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let accent = theme.accent_orange;
    let hover = mix_rgb(accent, 0xffffffff, 0.22);
    let pressed = mix_rgb(accent, 0x000000ff, 0.22);
    activatable(id, on_activate)
        .px(px(if compact { 10.0 } else { 14.0 }))
        .py(px(if compact { 5.0 } else { 7.0 }))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(accent))
        .bg(rgba(accent))
        .settings_font(TextRole::Control)
        .text_color(rgba(0xffffffff))
        .hover(move |style| style.bg(rgba(hover)).border_color(rgba(hover)))
        .active(move |style| style.bg(rgba(pressed)).border_color(rgba(pressed)))
        .focus(move |style| style.border_color(rgba(with_alpha(theme.text_primary, 0xe0))))
        .child(label)
        .into_any_element()
}

fn activatable(
    id: &'static str,
    on_activate: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<Div> {
    let on_activate = Rc::new(on_activate);
    let on_click = on_activate.clone();
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .cursor_pointer()
        .tab_index(0)
        .on_click(move |event, window, cx| on_click(event, window, cx))
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                on_activate(&ClickEvent::default(), window, cx);
                cx.stop_propagation();
            }
        })
}
