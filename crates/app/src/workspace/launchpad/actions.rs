use super::layout::{ACTION_CARD_HEIGHT, ACTION_CARD_WIDTH};
use super::{launchpad_logo_shadow, launchpad_shadow, with_alpha};
use crate::assets::APP_ICON_ASSET;
use gpui::{div, img, prelude::*, px, rgba, Div, FontWeight, ObjectFit};
use ui::Theme;

pub(super) fn launchpad_brand(theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(14.0))
                .child(
                    div()
                        .w(px(54.0))
                        .h(px(54.0))
                        .rounded(px(14.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(rgba(with_alpha(theme.edge_highlight, 0x70)))
                        .shadow(launchpad_logo_shadow(theme))
                        .bg(rgba(with_alpha(theme.panel_high, 0xa0)))
                        .child(img(APP_ICON_ASSET).size_full().object_fit(ObjectFit::Cover)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(28.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgba(theme.text_primary))
                                .child("MULTISOUL"),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(rgba(theme.text_tertiary))
                                .child("Workspace"),
                        ),
                ),
        )
        .child(
            div()
                .mt(px(42.0))
                .text_size(px(11.0))
                .font_family("Menlo")
                .text_color(rgba(theme.accent_cyan))
                .child("INFINITE CANVAS"),
        )
        .child(
            div()
                .mt(px(14.0))
                .w(px(520.0))
                .text_size(px(52.0))
                .line_height(px(54.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child("Start where your work is still alive."),
        )
        .child(
            div()
                .mt(px(14.0))
                .w(px(460.0))
                .text_size(px(15.0))
                .line_height(px(23.0))
                .text_color(rgba(theme.text_secondary))
                .child(
                    "Open a project and return to the same canvas rhythm: terminals, agents, previews, and workspace state held in one spatial surface.",
                ),
        )
}

pub(super) fn action_card(
    theme: Theme,
    icon: &'static str,
    label: &'static str,
    meta: &'static str,
    enabled: bool,
) -> Div {
    let text_color = if enabled {
        theme.text_primary
    } else {
        theme.text_secondary
    };
    let stroke = if enabled {
        with_alpha(theme.accent_orange, 0x88)
    } else {
        with_alpha(theme.edge_highlight, 0x28)
    };
    let fill = if enabled {
        with_alpha(theme.accent_orange, 0x18)
    } else {
        with_alpha(theme.panel, 0x88)
    };

    div()
        .w(px(ACTION_CARD_WIDTH))
        .h(px(ACTION_CARD_HEIGHT))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(7.0))
        .px(px(16.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(stroke))
        .shadow(launchpad_shadow(theme))
        .bg(rgba(fill))
        .text_color(rgba(text_color))
        .cursor_pointer()
        .hover(move |style| {
            if enabled {
                style.bg(rgba(with_alpha(theme.accent_orange, 0x24)))
            } else {
                style.bg(rgba(with_alpha(theme.panel_high, 0x98)))
            }
        })
        .child(
            div()
                .text_size(px(19.0))
                .text_color(rgba(if enabled {
                    theme.accent_orange
                } else {
                    theme.text_secondary
                }))
                .child(icon),
        )
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(FontWeight::BOLD)
                .child(label),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgba(theme.text_tertiary))
                .child(meta),
        )
}
