//! 特效库网格：与壁纸库同样的自适应列；静止显示定格帧，悬停才播放。

use super::current::style_of;
use super::page::EffectsPageData;
use super::preview::{preview_scene, Preview, PreviewTime};
use super::state::GRID_STYLES;
use crate::app::effects_actions::{style_chips, style_name};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::wallpaper::layout::WallpaperLayout;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, KeyDownEvent, Stateful};
use ui::component::{Icon, IconName};
use ui::Theme;

pub(super) fn grid_section(
    theme: Theme,
    data: &EffectsPageData,
    layout: &WallpaperLayout,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let Some(target) = data.target.clone() else {
        return div();
    };
    let selected = data.state.selected_style(&target).to_owned();
    let dim = data.state.follows_workspace(&target);
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .settings_font(TextRole::Headline)
                .child(ui::i18n::text("effects.library")),
        )
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("effects.library_hint")),
        )
        .child(
            div()
                .settings_text(TextRole::Eyebrow, theme)
                .child(ui::i18n::text("effects.built_in")),
        )
        .child(
            div()
                .id("effects-grid")
                .debug_selector(|| "effects-grid".into())
                .w(px(layout.columns as f32
                    * (layout.cell_width + WallpaperLayout::GAP)
                    - WallpaperLayout::GAP))
                .flex()
                .flex_wrap()
                .gap(px(WallpaperLayout::GAP))
                .opacity(if dim { 0.55 } else { 1.0 })
                .children(GRID_STYLES.iter().enumerate().map(|(index, id)| {
                    let spec = CellSpec {
                        index,
                        id,
                        selected: *id == selected,
                        width: layout.cell_width,
                        disabled: dim,
                    };
                    cell(theme, data, spec, cx)
                })),
        )
}

struct CellSpec {
    index: usize,
    id: &'static str,
    selected: bool,
    width: f32,
    disabled: bool,
}

fn cell(
    theme: Theme,
    data: &EffectsPageData,
    spec: CellSpec,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let CellSpec {
        index,
        id,
        selected,
        width,
        disabled,
    } = spec;
    let hovered = data.hover == Some(index) && !data.reduce_motion;
    let border = if selected {
        rgba(theme.accent_blue)
    } else {
        rgba((theme.edge_highlight & 0xffffff00) | 0x40)
    };
    let style = style_of(id);
    let time = match (hovered, data.hover_since) {
        (true, Some(started)) => PreviewTime::Playing { started },
        _ => PreviewTime::Still,
    };
    let frame = div()
        .id(("effects-preview", index))
        .debug_selector(move || format!("effects-preview-{id}"))
        .relative()
        .w(px(width - 10.0))
        .h(px((width - 10.0) * 9.0 / 16.0))
        .overflow_hidden()
        .rounded(px(8.0))
        .border_1()
        .border_color(border)
        .bg(rgba((theme.panel_high & 0xffffff00) | 0x80))
        .child(preview_scene(Preview {
            theme,
            style,
            time,
            width: width - 12.0,
            radius: 7.0,
            speed: data.state.speed.multiplier(),
        }))
        .when(id == "random" && !hovered, |frame| {
            frame.child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(IconName::Replace)
                            .size(px(26.0))
                            .text_color(rgba(theme.text_secondary)),
                    ),
            )
        })
        .when(!hovered && !data.reduce_motion && id != "random", |frame| {
            frame.child(
                div()
                    .absolute()
                    .left(px(8.0))
                    .bottom(px(8.0))
                    .h(px(20.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(10.0))
                    .bg(rgba(0x00000073))
                    .text_color(rgba(0xffffffff))
                    .text_size(px(11.0))
                    .child("▶")
                    .child(ui::i18n::text("effects.preview")),
            )
        })
        .when(selected, |frame| {
            frame.child(
                div()
                    .absolute()
                    .right(px(7.0))
                    .top(px(7.0))
                    .size(px(22.0))
                    .rounded(px(11.0))
                    .bg(rgba(theme.accent_blue))
                    .text_color(rgba(0xffffffff))
                    .text_size(px(13.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("✓"),
            )
        });
    let duration = style.map(|style| {
        format!(
            "{:.2}s",
            style.timing(1.0).flight.as_secs_f32() + style.timing(1.0).impact.as_secs_f32()
        )
    });
    div()
        .id(("effects-cell", index))
        .debug_selector(move || format!("effects-cell-{id}"))
        .w(px(width))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .when(!disabled, |cell| cell.cursor_pointer())
        .tab_index(if disabled { -1 } else { 0 })
        .rounded(px(8.0))
        .p(px(4.0))
        .border_1()
        .border_color(rgba(0x00000000))
        .hover(move |style| style.bg(rgba(theme.panel_high)))
        .active(move |style| style.bg(rgba(theme.node_stroke)))
        .focus(move |style| {
            style
                .bg(rgba(theme.panel_high))
                .border_1()
                .border_color(rgba(theme.accent_blue))
        })
        .on_hover(
            cx.listener(move |app, entered: &bool, _, cx| {
                app.set_effects_hover(index, *entered, cx)
            }),
        )
        .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                app.pick_effect_style(id, cx);
                cx.stop_propagation();
            }
        }))
        .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| app.pick_effect_style(id, cx)))
        .child(frame)
        .child(
            div()
                .px(px(4.0))
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .settings_font(TextRole::Body)
                        .text_color(rgba(if selected {
                            theme.accent_blue
                        } else {
                            theme.text_primary
                        }))
                        .truncate()
                        .child(style_name(id)),
                )
                .child(
                    div()
                        .settings_text(TextRole::Caption, theme)
                        .child(duration.unwrap_or_default()),
                ),
        )
        .child(
            div()
                .px(px(4.0))
                .mt(px(-4.0))
                .settings_text(TextRole::Caption, theme)
                .child(if id == "meteor" {
                    ui::i18n::text("effects.badge_original")
                } else {
                    style_chips(id)[0]
                }),
        )
}
