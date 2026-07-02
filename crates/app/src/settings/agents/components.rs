use gpui::{div, prelude::*, px, rgba, Div, FontWeight};
use ui::Theme;

pub(super) fn section(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .pt(px(18.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.accent_cyan))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(hint),
                ),
        )
}

pub(super) fn action_button(_theme: Theme, label: &'static str, accent: u32) -> Div {
    div()
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
        .child(label)
}

pub(super) fn status_chip(theme: Theme, enabled: bool) -> Div {
    div()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(999.0))
        .bg(rgba(with_alpha(
            if enabled {
                theme.accent_cyan
            } else {
                theme.destructive
            },
            0x18,
        )))
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(if enabled {
            theme.accent_cyan
        } else {
            theme.destructive
        }))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xd2))))
        .child(if enabled { "Enabled" } else { "Disabled" })
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
