use super::layout::LaunchpadLayout;
use super::{launchpad_shadow, with_alpha};
use gpui::{div, prelude::*, px, rgba, Div, FontWeight};
use ui::Theme;

pub(super) fn preview_cluster(theme: Theme, layout: LaunchpadLayout) -> Div {
    div()
        .absolute()
        .left(px(layout.preview_left))
        .top(px(layout.preview_top))
        .w(px(layout.preview_width))
        .h(px(layout.preview_height))
        .child(terminal_preview(theme))
        .child(agent_preview(theme))
        .child(document_preview(theme))
        .child(workspace_shape(theme))
}

fn terminal_preview(theme: Theme) -> Div {
    preview_card(theme, 22.0, 38.0, 342.0, 228.0)
        .child(preview_title(
            theme,
            "Terminal",
            "sample",
            theme.accent_cyan,
        ))
        .child(
            preview_body()
                .child(preview_line(
                    theme,
                    "~/multisoul $ cargo test -p runtime",
                    LineTone::Dim,
                ))
                .child(preview_line(
                    theme,
                    "test result: ok. 42 passed",
                    LineTone::Good,
                ))
                .child(preview_line(
                    theme,
                    "~/multisoul $ cargo check -p app",
                    LineTone::Dim,
                ))
                .child(preview_line(
                    theme,
                    "checking multisoul-app v0.1.0",
                    LineTone::Hot,
                )),
        )
}

fn agent_preview(theme: Theme) -> Div {
    preview_card(theme, 220.0, 202.0, 300.0, 210.0)
        .child(preview_title(
            theme,
            "Codex Agent",
            "context",
            theme.accent_violet,
        ))
        .child(
            preview_body()
                .child(preview_line(theme, "reading launchpad.rs", LineTone::Dim))
                .child(preview_line(theme, "mapping canvas state", LineTone::Base))
                .child(preview_line(theme, "ready for UI pass", LineTone::Good)),
        )
}

fn document_preview(theme: Theme) -> Div {
    preview_card(theme, 82.0, 386.0, 360.0, 188.0)
        .child(preview_title(
            theme,
            "Preview",
            "artifact",
            theme.accent_warm,
        ))
        .child(
            preview_body()
                .child(preview_line(theme, "ARCHITECTURE.md", LineTone::Base))
                .child(preview_line(
                    theme,
                    "workspace launchpad, flow, persistence",
                    LineTone::Dim,
                ))
                .child(preview_line(
                    theme,
                    "storage boundaries clean",
                    LineTone::Dim,
                )),
        )
}

fn workspace_shape(theme: Theme) -> Div {
    div()
        .absolute()
        .right(px(0.0))
        .bottom(px(0.0))
        .w(px(278.0))
        .h(px(118.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x30)))
        .shadow(launchpad_shadow(theme))
        .bg(rgba(with_alpha(theme.panel, 0xd0)))
        .p(px(14.0))
        .child(
            div()
                .mb(px(12.0))
                .text_size(px(12.0))
                .text_color(rgba(theme.text_tertiary))
                .child("Workspace shape"),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(10.0))
                .child(shape_tile(theme, "Term", "shell"))
                .child(shape_tile(theme, "Agent", "codex"))
                .child(shape_tile(theme, "Preview", "artifact")),
        )
}

fn preview_card(theme: Theme, left: f32, top: f32, width: f32, height: f32) -> Div {
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(width))
        .h(px(height))
        .overflow_hidden()
        .rounded(px(14.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x32)))
        .shadow(launchpad_shadow(theme))
        .bg(rgba(with_alpha(theme.panel, 0xd8)))
}

fn preview_title(theme: Theme, title: &'static str, badge: &'static str, badge_color: u32) -> Div {
    div()
        .h(px(42.0))
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(14.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x18)))
        .text_size(px(12.0))
        .text_color(rgba(theme.text_secondary))
        .child(title)
        .child(div().text_color(rgba(badge_color)).child(badge))
}

fn preview_body() -> Div {
    div()
        .p(px(14.0))
        .flex()
        .flex_col()
        .gap(px(7.0))
        .font_family("Menlo")
        .text_size(px(12.0))
}

fn preview_line(theme: Theme, text: &'static str, tone: LineTone) -> Div {
    let color = match tone {
        LineTone::Base => theme.term_fg,
        LineTone::Dim => with_alpha(theme.term_fg, 0x66),
        LineTone::Good => theme.accent_green,
        LineTone::Hot => theme.accent_orange,
    };

    div().text_color(rgba(color)).child(text)
}

fn shape_tile(theme: Theme, title: &'static str, meta: &'static str) -> Div {
    div()
        .flex_1()
        .h(px(58.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x14)))
        .bg(rgba(with_alpha(theme.graphite, 0x66)))
        .p(px(10.0))
        .child(
            div()
                .mb(px(4.0))
                .text_size(px(12.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_secondary))
                .child(title),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgba(theme.text_tertiary))
                .child(meta),
        )
}

#[derive(Clone, Copy)]
enum LineTone {
    Base,
    Dim,
    Good,
    Hot,
}
