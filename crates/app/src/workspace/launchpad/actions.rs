use super::layout::{ACTION_CARD_HEIGHT, ACTION_CARD_WIDTH};
use super::{launchpad_shadow, with_alpha};
use crate::assets::APP_LOGO_ASSET;
use gpui::{div, img, prelude::*, px, rgba, Div, FontWeight, ObjectFit};
use ui::Theme;

pub(super) fn launchpad_brand(theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .w(px(28.0))
                        .h(px(28.0))
                        .rounded(px(8.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(rgba(with_alpha(theme.node_stroke, 0xa8)))
                        .bg(rgba(theme.panel_high))
                        .child(img(APP_LOGO_ASSET).size_full().object_fit(ObjectFit::Cover)),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_family("Menlo")
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.accent_blue))
                        .child("NORMA / WORKSPACE"),
                ),
        )
        .child(
            div()
                .mt(px(16.0))
                .w(px(490.0))
                .text_size(px(42.0))
                .line_height(px(45.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child("Pick up where your workspace is still alive."),
        )
        .child(
            div()
                .mt(px(10.0))
                .w(px(456.0))
                .text_size(px(15.0))
                .line_height(px(22.0))
                .text_color(rgba(theme.text_secondary))
                .child(
                    "Open a project and return to terminals, agents, previews, and every live node in one spatial surface.",
                ),
        )
}

pub(super) fn action_card(
    theme: Theme,
    icon: &'static str,
    label: &'static str,
    _meta: &'static str,
    enabled: bool,
) -> Div {
    let text_color = if enabled {
        0xffffffff
    } else {
        theme.text_primary
    };
    let fill = if enabled {
        theme.graphite
    } else {
        with_alpha(theme.panel_high, 0xdc)
    };

    div()
        .w(px(ACTION_CARD_WIDTH))
        .h(px(ACTION_CARD_HEIGHT))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .px(px(14.0))
        .rounded(px(22.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.node_stroke, 0xb8)))
        .shadow(launchpad_shadow(theme))
        .bg(rgba(fill))
        .text_color(rgba(text_color))
        .cursor_pointer()
        .hover(move |style| {
            if enabled {
                style.bg(rgba(with_alpha(theme.graphite, 0xe6)))
            } else {
                style.bg(rgba(with_alpha(theme.panel_high, 0x98)))
            }
        })
        .child(
            div()
                .w(px(22.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.0))
                .bg(rgba(if enabled { 0xffffff1f } else { theme.canvas_to }))
                .text_size(px(15.0))
                .text_color(rgba(text_color))
                .child(icon),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::BOLD)
                .child(label),
        )
}
