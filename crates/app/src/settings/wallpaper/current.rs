//! The current choice uses the same bounded still-frame cache as the library.
use crate::settings::typography::{SettingsText, TextRole};

use super::{cells::preview_thumbnail, layout::WallpaperLayout, state::WallpaperSelection};
use super::{preview::PreviewCache, state::WallpaperSettingsState};
use crate::app::{
    wallpaper_resolvers::{builtin_wallpapers, resolve_builtin_asset, WallpaperKind},
    MultisoulApp,
};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::Theme;

pub(super) fn current_wallpaper(
    theme: Theme,
    state: &WallpaperSettingsState,
    previews: &mut PreviewCache,
    layout: &WallpaperLayout,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let pending = match &state.selection {
        WallpaperSelection::Builtin { rel } => crate::media::view::wallpaper_pending(rel, cx),
        WallpaperSelection::Custom { .. } => false,
    };
    let footer = state
        .preferences
        .status
        .as_ref()
        .map(|status| {
            if pending && status == &ui::i18n::message("norma.saved", &[]) {
                ui::i18n::text("media.wallpaper_selection_saved").to_owned()
            } else {
                status.to_string()
            }
        })
        .or_else(|| (!pending).then(|| ui::i18n::text("settings.auto_save").to_owned()));
    let (name, kind, path) = match &state.selection {
        WallpaperSelection::Builtin { rel } => {
            let (_, name, kind) = builtin_wallpapers()
                .iter()
                .find(|(key, _, _)| key == rel)
                .copied()
                .unwrap_or((
                    rel,
                    ui::i18n::text("settings.wallpaper_default"),
                    WallpaperKind::Static,
                ));
            (
                super::cells::builtin_name(name).to_string(),
                kind,
                resolve_builtin_asset(rel)
                    .or_else(|| crate::app::wallpaper_resolvers::builtin_preview_asset(rel)),
            )
        }
        WallpaperSelection::Custom { filename } => {
            let path = super::dirs::custom_wallpapers_dir().map(|dir| dir.join(filename));
            let kind = std::path::Path::new(filename)
                .extension()
                .and_then(|ext| ext.to_str())
                .and_then(WallpaperKind::from_extension)
                .unwrap_or(WallpaperKind::Static);
            (filename.clone(), kind, path)
        }
    };
    let preview_width = if layout.stacked_preview {
        layout.width - 34.0
    } else {
        272.0
    };
    let description = ui::i18n::text("settings.global_wallpaper_hint");
    div()
        .flex()
        .items_center()
        .gap(px(20.0))
        .p(px(16.0))
        .when(layout.stacked_preview, |card| card.flex_col().items_start())
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba((theme.node_stroke & 0xffffff00) | 0x60))
        .bg(rgba(theme.panel_high))
        .child(
            div()
                .flex_none()
                .w(px(preview_width))
                .h(px(preview_width * 9.0 / 16.0))
                .rounded(px(12.0))
                .bg(rgba(theme.panel))
                .child(preview_thumbnail(kind, path, previews, 12.0, cx)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .when(layout.stacked_preview, |info| info.w_full())
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .settings_text(TextRole::Eyebrow, theme)
                        .child(ui::i18n::text(if pending {
                            "media.wallpaper_pending"
                        } else {
                            "settings.current_wallpaper"
                        })),
                )
                .when(pending, |info| {
                    info.child(
                        div()
                            .id("wallpaper-pending-selection")
                            .debug_selector(|| "wallpaper-pending-selection".into())
                            .settings_text(TextRole::Helper, theme)
                            .child(ui::i18n::text("media.wallpaper_auto_apply")),
                    )
                })
                .child(
                    div()
                        .settings_font(TextRole::Headline)
                        .truncate()
                        .child(name),
                )
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .child(match kind {
                            WallpaperKind::Static => ui::i18n::text("settings.still_image"),
                            WallpaperKind::Gif | WallpaperKind::Video => {
                                ui::i18n::text("settings.animated_wallpaper")
                            }
                        }),
                )
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .child(description),
                )
                .when_some(
                    match &state.selection {
                        WallpaperSelection::Builtin { rel } => {
                            crate::media::view::status(rel, theme, cx)
                        }
                        WallpaperSelection::Custom { .. } => None,
                    },
                    |info, status| info.child(status),
                )
                .when_some(footer, |info, message| {
                    info.child(
                        div()
                            .id("wallpaper-save-feedback")
                            .debug_selector(|| "wallpaper-save-feedback".into())
                            .min_h(px(18.0))
                            .settings_text(TextRole::Caption, theme)
                            .child(message),
                    )
                }),
        )
}
