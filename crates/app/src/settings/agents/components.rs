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
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
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
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
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
        .px(px(14.0))
        .py(px(12.0))
        .text_size(px(13.0))
        .line_height(px(20.0))
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
        .px(px(14.0))
        .py(px(12.0))
        .text_size(px(13.0))
        .text_color(rgba(theme.text_tertiary))
        .child(message)
}

pub(super) fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
