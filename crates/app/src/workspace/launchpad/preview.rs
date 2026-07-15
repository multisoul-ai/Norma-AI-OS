use super::layout::LaunchpadLayout;
use super::{launchpad_shadow, with_alpha};
use gpui::{div, prelude::*, px, rgba, Div, FontWeight};
use ui::Theme;

pub(super) fn workspace_preview(theme: Theme, layout: LaunchpadLayout) -> impl IntoElement {
    div()
        .id("launchpad-workspace-preview")
        .absolute()
        .left(px(layout.preview_left))
        .top(px(layout.preview_top))
        .w(px(layout.preview_width))
        .h(px(layout.preview_height))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(28.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.node_stroke, 0xd8)))
        .shadow(launchpad_shadow(theme))
        .bg(rgba(with_alpha(theme.panel_high, 0xb0)))
        .child(preview_titlebar(theme))
        .child(
            div()
                .relative()
                .flex_1()
                .min_h(px(0.0))
                .bg(rgba(with_alpha(theme.canvas_to, 0x78)))
                .child(preview_node(
                    theme,
                    PreviewNodeSpec {
                        left: 0.28,
                        top: 0.40,
                        width: 0.54,
                        height: 0.30,
                        label: "TERMINAL",
                        body: "~/norma  cargo test",
                        accent: theme.accent_cyan,
                    },
                ))
                .child(preview_node(
                    theme,
                    PreviewNodeSpec {
                        left: 0.49,
                        top: 0.22,
                        width: 0.43,
                        height: 0.27,
                        label: "CODEX",
                        body: "Refining canvas…",
                        accent: theme.accent_violet,
                    },
                ))
                .child(preview_node(
                    theme,
                    PreviewNodeSpec {
                        left: 0.40,
                        top: 0.58,
                        width: 0.48,
                        height: 0.24,
                        label: "PREVIEW",
                        body: "localhost:3000",
                        accent: theme.accent_green,
                    },
                ))
                .child(
                    div()
                        .absolute()
                        .left(px(22.0))
                        .right(px(22.0))
                        .bottom(px(18.0))
                        .h(px(42.0))
                        .rounded(px(21.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.edge_highlight, 0x34)))
                        .bg(rgba(with_alpha(theme.graphite, 0xde)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(18.0))
                        .text_size(px(12.0))
                        .text_color(rgba(0xffffffc8))
                        .child("⌖")
                        .child("+")
                        .child("›_")
                        .child("✦")
                        .child("◎"),
                ),
        )
}

fn preview_titlebar(theme: Theme) -> Div {
    div()
        .h(px(46.0))
        .flex()
        .items_center()
        .justify_between()
        .px(px(18.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.graphite, 0x12)))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_secondary))
                .child("Workspace preview"),
        )
        .child(
            div()
                .flex()
                .gap(px(6.0))
                .child(preview_dot(theme.destructive))
                .child(preview_dot(theme.accent_warm))
                .child(preview_dot(theme.accent_green)),
        )
}

fn preview_dot(color: u32) -> Div {
    div().w(px(8.0)).h(px(8.0)).rounded(px(4.0)).bg(rgba(color))
}

struct PreviewNodeSpec {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    label: &'static str,
    body: &'static str,
    accent: u32,
}

fn preview_node(theme: Theme, spec: PreviewNodeSpec) -> Div {
    div()
        .absolute()
        .left(gpui::relative(spec.left))
        .top(gpui::relative(spec.top))
        .w(gpui::relative(spec.width))
        .h(gpui::relative(spec.height))
        .min_w(px(190.0))
        .min_h(px(120.0))
        .overflow_hidden()
        .rounded(px(18.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.node_stroke, 0xbc)))
        .bg(rgba(with_alpha(theme.panel_high, 0xd0)))
        .child(
            div()
                .h(px(34.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .text_size(px(10.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_secondary))
                .child(preview_dot(spec.accent))
                .child(spec.label),
        )
        .child(
            div()
                .mx(px(8.0))
                .mb(px(8.0))
                .p(px(12.0))
                .rounded(px(12.0))
                .bg(rgba(with_alpha(theme.term_bg, 0xe8)))
                .font_family("Menlo")
                .text_size(px(10.0))
                .text_color(rgba(theme.term_fg))
                .child(spec.body),
        )
}
