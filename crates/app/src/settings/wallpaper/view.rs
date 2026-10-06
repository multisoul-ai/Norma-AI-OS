//! Wallpaper page: target → current preview → shared library.
use crate::settings::typography::{control_label, SettingsText, TextRole};

use super::action::WallpaperAction;
use super::{cells, layout::WallpaperLayout, preview::PreviewCache, state::WallpaperSettingsState};
use crate::app::{wallpaper_resolvers::builtin_wallpapers, MultisoulApp};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::{
    component::{button::Button, Disableable},
    Theme,
};

pub(crate) fn wallpaper_settings(
    theme: Theme,
    state: &WallpaperSettingsState,
    previews: &mut PreviewCache,
    available_width: f32,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let layout = WallpaperLayout::for_width(available_width);
    let selected = super::scope_view::selected_state(state);
    let state = &selected;
    let busy = state.scoped.busy || state.scoped.delete_prompt.is_some();
    let builtins = builtin_wallpapers();
    let customs = state.scoped.customs.clone();
    div()
        .w(px(layout.width))
        .flex()
        .flex_col()
        .gap(px(24.0))
        .text_color(rgba(theme.text_primary))
        .child(super::scope_view::scope_controls(
            theme,
            &state.scoped,
            layout.stacked_preview,
            cx,
        ))
        .child(super::current::current_wallpaper(
            theme, state, previews, &layout, cx,
        ))
        .when(state.scoped.has_missing_selection(), |page| {
            page.child(super::scope_view::notices(theme, &state.scoped))
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .settings_font(TextRole::Headline)
                        .child(ui::i18n::text("settings.wallpaper_library")),
                )
                .child(
                    Button::new("wallpaper-import")
                        .child(control_label(ui::i18n::text("settings.add_wallpaper")))
                        .tooltip(ui::i18n::text("settings.import_formats"))
                        .disabled(busy)
                        .on_wallpaper_activate(
                            !busy,
                            cx.listener(|app, _, _, cx| app.import_wallpaper_file(cx)),
                        ),
                ),
        )
        .child(section_label(theme, ui::i18n::text("settings.built_in")))
        .child(
            div()
                .w(px(layout.columns as f32
                    * (layout.cell_width + WallpaperLayout::GAP)
                    - WallpaperLayout::GAP))
                .flex()
                .flex_wrap()
                .gap(px(WallpaperLayout::GAP))
                .opacity(if busy { 0.6 } else { 1.0 })
                .children(
                    builtins
                        .iter()
                        .enumerate()
                        .map(|(idx, &(rel, name, kind))| {
                            cells::builtin_cell(
                                theme,
                                state,
                                previews,
                                cells::BuiltinWallpaperCell {
                                    idx,
                                    rel,
                                    name,
                                    kind,
                                    width: layout.cell_width,
                                },
                                cx,
                            )
                        }),
                ),
        )
        .when(!customs.is_empty(), |page| {
            page.child(section_label(
                theme,
                ui::i18n::text("settings.your_wallpapers"),
            ))
            .child(
                div()
                    .w(px(layout.columns as f32
                        * (layout.cell_width + WallpaperLayout::GAP)
                        - WallpaperLayout::GAP))
                    .flex()
                    .flex_wrap()
                    .gap(px(WallpaperLayout::GAP))
                    .opacity(if busy { 0.6 } else { 1.0 })
                    .children(customs.iter().enumerate().map(|(idx, filename)| {
                        cells::custom_cell(
                            theme,
                            state,
                            previews,
                            idx,
                            filename,
                            layout.cell_width,
                            cx,
                        )
                    })),
            )
        })
}

fn section_label(theme: Theme, label: &str) -> Div {
    div()
        .settings_text(TextRole::Eyebrow, theme)
        .mb(px(-12.0))
        .child(label.to_owned())
}
