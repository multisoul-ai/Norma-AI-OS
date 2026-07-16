use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight, Stateful};
use ui::{Theme, ThemeId};

pub(super) fn theme_appearance(
    active_theme: ThemeId,
    theme: Theme,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(30.0))
        .child(theme_group("Light", false, active_theme, theme, cx))
        .child(theme_group("Dark", true, active_theme, theme, cx))
}

fn theme_group(
    label: &'static str,
    dark: bool,
    active_theme: ThemeId,
    theme: Theme,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let group_index = usize::from(dark);
    let selector = if dark {
        "settings-theme-group-dark"
    } else {
        "settings-theme-group-light"
    };
    let subtitle = if dark {
        "Focused palettes for low-light work"
    } else {
        "Bright palettes for everyday work"
    };
    div()
        .id(("settings-theme-group", group_index))
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(subtitle),
                ),
        )
        .child(
            div().w_full().flex().flex_row().gap(px(16.0)).children(
                ThemeId::ALL
                    .into_iter()
                    .filter(move |id| id.is_dark() == dark)
                    .map(|id| theme_card(id, active_theme, theme, cx)),
            ),
        )
}

fn theme_card(
    id: ThemeId,
    active_theme: ThemeId,
    chrome_theme: Theme,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let preview = Theme::for_id(id);
    let selected = id == active_theme;
    let selector = format!("settings-theme-{}", id.as_str());
    div()
        .id(("settings-theme", theme_index(id)))
        .debug_selector(move || selector.clone())
        .min_w(px(0.0))
        .flex_1()
        .h(px(214.0))
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba(if selected {
            preview.accent
        } else {
            with_alpha(chrome_theme.edge_highlight, 0x4c)
        }))
        .bg(rgba(with_alpha(chrome_theme.panel_high, 0xf0)))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(chrome_theme.panel_high, 0xfa))))
        .on_click(cx.listener(move |app, _, _window, cx| {
            app.set_theme(id, cx);
        }))
        .child(theme_preview(id, preview, chrome_theme))
        .child(theme_choice_row(id, selected, preview, chrome_theme))
}

fn theme_preview(id: ThemeId, theme: Theme, chrome_theme: Theme) -> Stateful<Div> {
    let selector = format!("settings-theme-{}-preview", id.as_str());
    div()
        .id(("settings-theme-preview", theme_index(id)))
        .debug_selector(move || selector.clone())
        .h(px(150.0))
        .flex_none()
        .p(px(14.0))
        .rounded_t(px(15.0))
        .bg(rgba(with_alpha(chrome_theme.panel, 0xf2)))
        .child(
            div()
                .w_full()
                .h_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(px(10.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.node_stroke, 0x88)))
                .bg(rgba(theme.canvas_from))
                .child(
                    div()
                        .h(px(22.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(7.0))
                        .bg(rgba(theme.title_bar))
                        .children(
                            [0xff6b7aff, 0xffbd2eff, 0x64e0b5ff]
                                .into_iter()
                                .map(|color| {
                                    div().w(px(5.0)).h(px(5.0)).rounded(px(3.0)).bg(rgba(color))
                                }),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .flex()
                        .child(
                            div()
                                .w(px(34.0))
                                .h_full()
                                .flex_none()
                                .bg(rgba(theme.canvas_to)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .gap(px(5.0))
                                .p(px(6.0))
                                .bg(rgba(theme.canvas_from))
                                .child(
                                    div()
                                        .h(px(8.0))
                                        .w(px(48.0))
                                        .rounded(px(4.0))
                                        .bg(rgba(with_alpha(theme.text_tertiary, 0x38))),
                                )
                                .child(preview_content_row(theme))
                                .child(preview_content_row(theme)),
                        ),
                ),
        )
}

fn preview_content_row(theme: Theme) -> Div {
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .gap(px(5.0))
        .child(
            div()
                .w(px(38.0))
                .h_full()
                .flex_none()
                .rounded(px(4.0))
                .bg(rgba(with_alpha(theme.node_fill, 0xc0))),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .rounded(px(4.0))
                .bg(rgba(with_alpha(theme.node_fill, 0xc0))),
        )
}

fn theme_choice_row(
    id: ThemeId,
    selected: bool,
    preview: Theme,
    chrome_theme: Theme,
) -> Stateful<Div> {
    let selector = format!("settings-theme-{}-choice", id.as_str());
    div()
        .id(("settings-theme-choice", theme_index(id)))
        .debug_selector(move || selector.clone())
        .h(px(62.0))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(10.0))
        .px(px(14.0))
        .rounded_b(px(15.0))
        .border_t_1()
        .border_color(rgba(with_alpha(chrome_theme.edge_highlight, 0x28)))
        .bg(rgba(if selected {
            with_alpha(preview.accent, 0x18)
        } else {
            with_alpha(chrome_theme.panel_high, 0xf0)
        }))
        .child(
            div()
                .w(px(20.0))
                .h(px(20.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.0))
                .border_1()
                .border_color(rgba(if selected {
                    preview.accent
                } else {
                    with_alpha(chrome_theme.text_tertiary, 0x72)
                }))
                .bg(rgba(if selected { preview.accent } else { 0x00000000 }))
                .text_size(px(12.0))
                .text_color(rgba(0xffffffff))
                .child(if selected { "✓" } else { "" }),
        )
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(14.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(chrome_theme.text_primary))
                .child(id.display_name()),
        )
}

fn theme_index(id: ThemeId) -> usize {
    match id {
        ThemeId::PaperBlue => 0,
        ThemeId::LinenSage => 1,
        ThemeId::SilverViolet => 2,
        ThemeId::SapphireNight => 3,
        ThemeId::MidnightViolet => 4,
        ThemeId::ForestMint => 5,
    }
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
#[path = "../../tests/unit/theme_settings_contract_tests.rs"]
mod theme_settings_contract_tests;
