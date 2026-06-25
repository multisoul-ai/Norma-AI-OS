use super::with_alpha;
use gpui::{div, prelude::*, px, rgba, Div, FontWeight};
use ui::Theme;

#[derive(Clone, Copy)]
enum ThemePreviewKind {
    Light,
    Dark,
    System,
}

pub(super) fn general_appearance(theme: Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(28.0))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(20.0))
                .child(theme_card(
                    theme,
                    "Light Mode",
                    ThemePreviewKind::Light,
                    true,
                ))
                .child(theme_card(
                    theme,
                    "Dark Mode",
                    ThemePreviewKind::Dark,
                    false,
                ))
                .child(theme_card(
                    theme,
                    "System Preferences",
                    ThemePreviewKind::System,
                    false,
                )),
        )
        .child(accent_colors(theme))
        .child(toggle_row(theme))
        .child(tables_view(theme))
}

fn theme_card(theme: Theme, label: &'static str, preview: ThemePreviewKind, selected: bool) -> Div {
    div()
        .w(px(232.0))
        .h(px(232.0))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(18.0))
        .border_1()
        .border_color(rgba(if selected {
            with_alpha(theme.accent_orange, 0xd8)
        } else {
            card_border(theme)
        }))
        .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(preview_thumbnail(theme, preview)),
        )
        .child(
            div()
                .h(px(60.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(14.0))
                .px(px(18.0))
                .bg(rgba(if selected {
                    with_alpha(theme.accent_orange, 0x18)
                } else {
                    with_alpha(theme.node_fill, 0xd8)
                }))
                .child(radio_dot(theme, selected))
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(theme.text_primary))
                        .child(label),
                ),
        )
}

fn preview_thumbnail(theme: Theme, kind: ThemePreviewKind) -> Div {
    match kind {
        ThemePreviewKind::Light => window_preview(
            theme,
            with_alpha(theme.panel_high, 0xd8),
            with_alpha(theme.node_fill, 0xf0),
            with_alpha(theme.edge_highlight, 0x22),
        ),
        ThemePreviewKind::Dark => window_preview(
            theme,
            theme.term_bg,
            with_alpha(theme.panel, 0xf2),
            with_alpha(theme.edge_highlight, 0x30),
        ),
        ThemePreviewKind::System => system_preview(theme),
    }
}

fn window_preview(theme: Theme, bg: u32, side: u32, block: u32) -> Div {
    div()
        .relative()
        .w(px(196.0))
        .h(px(126.0))
        .overflow_hidden()
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(card_border(theme)))
        .bg(rgba(bg))
        .child(window_dots(theme))
        .child(
            div()
                .h_full()
                .flex()
                .flex_row()
                .pt(px(23.0))
                .child(div().w(px(58.0)).h_full().bg(rgba(side)))
                .child(skeleton_grid(block)),
        )
}

fn system_preview(theme: Theme) -> Div {
    div()
        .w(px(196.0))
        .h(px(126.0))
        .overflow_hidden()
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(card_border(theme)))
        .flex()
        .flex_row()
        .child(
            div()
                .relative()
                .w(px(98.0))
                .h_full()
                .bg(rgba(with_alpha(theme.node_fill, 0xee)))
                .child(window_dots(theme))
                .child(skeleton_grid(with_alpha(theme.edge_highlight, 0x22))),
        )
        .child(
            div()
                .w(px(98.0))
                .h_full()
                .bg(rgba(theme.term_bg))
                .child(skeleton_grid(with_alpha(theme.edge_highlight, 0x30))),
        )
}

fn window_dots(theme: Theme) -> Div {
    div()
        .absolute()
        .top(px(13.0))
        .left(px(13.0))
        .flex()
        .gap(px(4.0))
        .child(color_circle(4.0, theme.destructive))
        .child(color_circle(4.0, theme.accent_warm))
        .child(color_circle(4.0, theme.accent_green))
}

fn skeleton_grid(block: u32) -> Div {
    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .gap(px(7.0))
        .p(px(12.0))
        .child(skeleton_bar(block, 70.0, 10.0))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(6.0))
                .child(skeleton_box(block, 46.0, 29.0))
                .child(skeleton_box(block, 86.0, 29.0)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(6.0))
                .child(skeleton_box(block, 46.0, 29.0))
                .child(skeleton_box(block, 86.0, 29.0)),
        )
}

fn skeleton_bar(color: u32, w: f32, h: f32) -> Div {
    div().w(px(w)).h(px(h)).rounded(px(4.0)).bg(rgba(color))
}

fn skeleton_box(color: u32, w: f32, h: f32) -> Div {
    div().w(px(w)).h(px(h)).rounded(px(5.0)).bg(rgba(color))
}

fn radio_dot(theme: Theme, selected: bool) -> Div {
    div()
        .w(px(22.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(11.0))
        .border_1()
        .border_color(rgba(if selected {
            theme.accent_orange
        } else {
            with_alpha(theme.edge_highlight, 0x3e)
        }))
        .bg(rgba(if selected {
            theme.accent_orange
        } else {
            with_alpha(theme.panel_high, 0xa8)
        }))
        .text_size(px(14.0))
        .text_color(rgba(theme.text_primary))
        .child(if selected { "✓" } else { "" })
}

fn accent_colors(theme: Theme) -> Div {
    section_divider(theme).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(section_label(
                theme,
                "Accent Colors",
                "Use system or custom accent colors",
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(13.0))
                    .child(color_circle(28.0, theme.panel_high))
                    .child(color_circle(28.0, theme.destructive))
                    .child(color_circle(28.0, theme.accent_green))
                    .child(color_circle(28.0, theme.accent_cyan))
                    .child(color_circle(28.0, theme.accent_violet))
                    .child(
                        div()
                            .ml(px(8.0))
                            .text_size(px(13.0))
                            .text_color(rgba(theme.text_tertiary))
                            .child("Custom Color"),
                    )
                    .child(color_chip(theme))
                    .child(selected_accent(theme)),
            ),
    )
}

fn color_circle(size: f32, color: u32) -> Div {
    div()
        .w(px(size))
        .h(px(size))
        .rounded(px(size * 0.5))
        .bg(rgba(color))
}

fn color_chip(theme: Theme) -> Div {
    div()
        .h(px(34.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(7.0))
        .border_1()
        .border_color(rgba(card_border(theme)))
        .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
        .text_size(px(14.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(theme.text_secondary))
        .child("# 4146F8")
}

fn selected_accent(theme: Theme) -> Div {
    div()
        .w(px(36.0))
        .h(px(36.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(18.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.accent_violet, 0x60)))
        .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
        .child(color_circle(28.0, theme.accent_violet))
}

fn toggle_row(theme: Theme) -> Div {
    section_divider(theme).child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(section_label(
                theme,
                "Show Animations",
                "Enable or disable UI animations",
            ))
            .child(toggle_switch(theme, true)),
    )
}

fn toggle_switch(theme: Theme, on: bool) -> Div {
    div()
        .w(px(46.0))
        .h(px(26.0))
        .flex()
        .items_center()
        .justify_end()
        .p(px(3.0))
        .rounded(px(13.0))
        .bg(rgba(if on {
            theme.accent_cyan
        } else {
            with_alpha(theme.panel_high, 0xc0)
        }))
        .child(
            div()
                .w(px(20.0))
                .h(px(20.0))
                .rounded(px(10.0))
                .bg(rgba(theme.text_primary)),
        )
}

fn tables_view(theme: Theme) -> Div {
    section_divider(theme)
        .flex()
        .flex_col()
        .gap(px(22.0))
        .child(section_label(
            theme,
            "Tables View",
            "Customize how tables are displayed in your app",
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(20.0))
                .child(table_preview(theme, false))
                .child(table_preview(theme, true)),
        )
}

fn table_preview(theme: Theme, selected: bool) -> Div {
    div()
        .w(px(246.0))
        .h(px(168.0))
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba(if selected {
            with_alpha(theme.accent_cyan, 0xd8)
        } else {
            card_border(theme)
        }))
        .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
        .p(px(14.0))
        .child(
            div()
                .h_full()
                .rounded(px(10.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x20)))
                .p(px(10.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgba(theme.text_tertiary))
                                .child("Table View"),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .child(skeleton_bar(
                                    with_alpha(theme.edge_highlight, 0x24),
                                    34.0,
                                    12.0,
                                ))
                                .child(skeleton_bar(
                                    with_alpha(theme.edge_highlight, 0x24),
                                    34.0,
                                    12.0,
                                )),
                        ),
                )
                .child(table_rows(theme, selected)),
        )
}

fn table_rows(theme: Theme, dense: bool) -> Div {
    let row_h = if dense { 13.0 } else { 20.0 };
    let block = with_alpha(theme.edge_highlight, 0x16);
    div()
        .flex()
        .flex_col()
        .gap(px(if dense { 5.0 } else { 8.0 }))
        .children((0..5).map(move |_| {
            div()
                .flex()
                .flex_row()
                .gap(px(8.0))
                .child(skeleton_box(block, 48.0, row_h))
                .child(skeleton_box(block, 76.0, row_h))
                .child(skeleton_box(block, 88.0, row_h))
        }))
}

fn section_divider(theme: Theme) -> Div {
    div()
        .w_full()
        .pt(px(24.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
}

fn section_label(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(18.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child(title),
        )
        .child(
            div()
                .text_size(px(16.0))
                .text_color(rgba(theme.text_tertiary))
                .child(hint),
        )
}

fn card_border(theme: Theme) -> u32 {
    with_alpha(theme.edge_highlight, 0x2c)
}
