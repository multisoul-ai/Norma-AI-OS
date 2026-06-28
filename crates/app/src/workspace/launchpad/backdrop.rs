use super::layout::{LaunchpadLayout, TOOLBAR_HEIGHT, TOOLBAR_WIDTH};
use super::{launchpad_shadow, with_alpha};
use gpui::{
    div, img, linear_color_stop, linear_gradient, prelude::*, px, rgba, Div, ImageSource, ObjectFit,
};
use ui::Theme;

pub(super) fn launchpad_backdrop(theme: Theme, wallpaper: ImageSource) -> Div {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .overflow_hidden()
        .bg(rgba(theme.graphite))
        .child(
            img(wallpaper)
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(rgba(with_alpha(theme.graphite, 0xd6)), 0.0),
                    linear_color_stop(rgba(with_alpha(theme.graphite, 0x68)), 1.0),
                )),
        )
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(rgba(with_alpha(theme.graphite, 0x40)), 0.0),
                    linear_color_stop(rgba(with_alpha(theme.graphite, 0xb8)), 1.0),
                )),
        )
        .child(
            div()
                .absolute()
                .left(px(-160.0))
                .top(px(54.0))
                .w(px(520.0))
                .h(px(520.0))
                .rounded(px(260.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.accent_cyan, 0x28))),
        )
        .child(
            div()
                .absolute()
                .right(px(-180.0))
                .bottom(px(-120.0))
                .w(px(620.0))
                .h(px(620.0))
                .rounded(px(310.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.accent_orange, 0x30))),
        )
        .child(
            div()
                .absolute()
                .left(px(90.0))
                .right(px(90.0))
                .bottom(px(84.0))
                .h(px(1.0))
                .bg(rgba(with_alpha(theme.accent_cyan, 0x22))),
        )
}

pub(super) fn toolbar_preview(theme: Theme, layout: LaunchpadLayout) -> Div {
    div()
        .absolute()
        .top(px(layout.toolbar_top))
        .left(px(layout.toolbar_left))
        .w(px(TOOLBAR_WIDTH))
        .h(px(TOOLBAR_HEIGHT))
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .px(px(12.0))
        .rounded(px(23.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x34)))
        .shadow(launchpad_shadow(theme))
        .bg(rgba(with_alpha(theme.panel, 0xdc)))
        .child(toolbar_chip(theme, "CANVAS", true))
        .child(toolbar_chip(theme, "+", false))
        .child(toolbar_chip(theme, "Claude Code", false))
        .child(toolbar_chip(theme, "Codex", false))
        .child(toolbar_chip(theme, "Preview", false))
        .child(toolbar_chip(theme, "Settings", false))
}

fn toolbar_chip(theme: Theme, label: &'static str, active: bool) -> Div {
    let fill = if active {
        with_alpha(theme.accent_orange, 0x36)
    } else {
        with_alpha(theme.edge_highlight, 0x08)
    };
    let stroke = if active {
        with_alpha(theme.accent_orange, 0x78)
    } else {
        with_alpha(theme.edge_highlight, 0x20)
    };
    let text = if active {
        theme.accent_orange
    } else {
        theme.text_primary
    };

    div()
        .w(px(toolbar_chip_width(label)))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(stroke))
        .bg(rgba(fill))
        .text_color(rgba(text))
        .text_size(px(11.0))
        .child(label)
}

fn toolbar_chip_width(label: &str) -> f32 {
    let text_width = label.chars().count() as f32 * 6.4;
    (text_width + 18.0).max(28.0).ceil()
}
