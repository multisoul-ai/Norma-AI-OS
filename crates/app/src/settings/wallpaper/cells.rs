//! Wallpaper library tiles: static previews, selection feedback and shared-asset deletion.
use crate::settings::typography::{SettingsText, TextRole};

use super::action::WallpaperAction;
use super::dirs::custom_wallpapers_dir;
use super::preview::PreviewCache;
use super::state::WallpaperSettingsState;
use crate::app::wallpaper_resolvers::{builtin_wallpapers, resolve_builtin_asset, WallpaperKind};
use crate::app::MultisoulApp;
use gpui::{
    div, img, prelude::*, px, rgba, ClickEvent, Context, Div, ImageSource, MouseButton, ObjectFit,
    Stateful,
};
use std::path::PathBuf;
use ui::component::{
    button::{Button, ButtonVariants},
    Disableable,
};
use ui::Theme;

pub(super) struct BuiltinWallpaperCell {
    pub idx: usize,
    pub rel: &'static str,
    pub name: &'static str,
    pub kind: WallpaperKind,
    pub width: f32,
}

pub(super) fn builtin_name(name: &str) -> &str {
    match name {
        "Default" => ui::i18n::text("settings.wallpaper_default"),
        "Aurora" => ui::i18n::text("settings.wallpaper_aurora"),
        "Nebula" => ui::i18n::text("settings.wallpaper_nebula"),
        "Drift" => ui::i18n::text("settings.wallpaper_drift"),
        "Pulse" => ui::i18n::text("settings.wallpaper_pulse"),
        "Tide" => ui::i18n::text("settings.wallpaper_tide"),
        "Ember" => ui::i18n::text("settings.wallpaper_ember"),
        "Monsoon" => ui::i18n::text("settings.wallpaper_monsoon"),
        "Abyss" => ui::i18n::text("settings.wallpaper_abyss"),
        "Halcyon" => ui::i18n::text("settings.wallpaper_halcyon"),
        _ => name,
    }
}

/// One grid cell for a builtin wallpaper.
pub(super) fn builtin_cell(
    theme: Theme,
    state: &WallpaperSettingsState,
    previews: &mut PreviewCache,
    cell: BuiltinWallpaperCell,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let BuiltinWallpaperCell {
        idx,
        rel,
        name,
        kind,
        width,
    } = cell;
    let name = builtin_name(name);
    let selected = state.is_selected_builtin(rel);
    div()
        .id(("wallpaper-thumb", idx))
        .w(px(width))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .cursor_pointer()
        .tab_index(
            if state.scoped.busy || state.scoped.delete_prompt.is_some() {
                -1
            } else {
                0
            },
        )
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
        .on_key_down(cx.listener(move |app, event: &gpui::KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                if !app.wallpaper_settings.scoped.busy
                    && app.wallpaper_settings.scoped.delete_prompt.is_none()
                {
                    app.request_wallpaper_selection(
                        super::state::WallpaperSelection::Builtin { rel },
                        cx,
                    );
                }
                cx.stop_propagation();
            }
        }))
        .on_click(cx.listener(move |app, _: &ClickEvent, _window, cx| {
            super::scope_view::request_builtin(app, rel, cx);
        }))
        .child(thumbnail_box(
            ThumbnailBoxContent {
                theme,
                kind,
                selected,
                width,
                thumbnail: preview_thumbnail(
                    kind,
                    resolve_builtin_asset(rel)
                        .or_else(|| crate::app::wallpaper_resolvers::builtin_preview_asset(rel)),
                    previews,
                    7.0,
                    cx,
                ),
                badge_id: idx,
                badge_selector: format!(
                    "settings-wallpaper-badge-{idx}-{}",
                    kind.badge_label().to_ascii_lowercase()
                ),
                delete: None,
            },
            cx,
        ))
        .child(thumbnail_label(theme, name, selected))
        .child(div().min_h(px(22.0)).when_some(
            crate::media::view::wallpaper_status(rel, theme, cx),
            |row, status| row.child(status),
        ))
}

/// One grid cell for a custom wallpaper. Custom cells expose a hover-only
/// delete button (id `("wallpaper-delete", idx)`); builtins never do.
pub(super) fn custom_cell(
    theme: Theme,
    state: &WallpaperSettingsState,
    previews: &mut PreviewCache,
    idx: usize,
    filename: &str,
    width: f32,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let selected = state.is_selected_custom(filename);
    let filename_owned = filename.to_string();
    let keyboard_filename = filename.to_string();
    let delete_filename = filename.to_string();
    let path = custom_wallpapers_dir().map(|d| d.join(filename));
    let kind = path
        .as_ref()
        .and_then(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .and_then(WallpaperKind::from_extension)
        })
        .unwrap_or(WallpaperKind::Static);
    let thumb = preview_thumbnail(kind, path, previews, 7.0, cx);

    div()
        .id(("wallpaper-thumb-custom", idx))
        .w(px(width))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .cursor_pointer()
        .tab_index(
            if state.scoped.busy || state.scoped.delete_prompt.is_some() {
                -1
            } else {
                0
            },
        )
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
        .on_key_down(cx.listener(move |app, event: &gpui::KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                if !app.wallpaper_settings.scoped.busy
                    && app.wallpaper_settings.scoped.delete_prompt.is_none()
                {
                    app.request_wallpaper_selection(
                        super::state::WallpaperSelection::Custom {
                            filename: keyboard_filename.clone(),
                        },
                        cx,
                    );
                }
                cx.stop_propagation();
            }
        }))
        .on_click(cx.listener(move |app, _: &ClickEvent, _window, cx| {
            if !app.wallpaper_settings.scoped.busy
                && app.wallpaper_settings.scoped.delete_prompt.is_none()
            {
                app.request_wallpaper_selection(
                    super::state::WallpaperSelection::Custom {
                        filename: filename_owned.clone(),
                    },
                    cx,
                );
            }
        }))
        .child(thumbnail_box(
            ThumbnailBoxContent {
                theme,
                kind,
                selected,
                width,
                thumbnail: thumb,
                badge_id: builtin_wallpapers().len() + idx,
                badge_selector: format!(
                    "settings-wallpaper-custom-badge-{idx}-{}",
                    kind.badge_label().to_ascii_lowercase()
                ),
                delete: Some((
                    idx,
                    delete_filename,
                    state.scoped.busy || state.scoped.delete_prompt.is_some(),
                )),
            },
            cx,
        ))
        .child(thumbnail_label(theme, filename, selected))
}

/// The fixed-size thumbnail frame. `delete` carries `(idx, filename)` for
/// custom cells so a hover delete affordance can be layered on top; `None`
/// for builtins (they are never deletable).
struct ThumbnailBoxContent {
    theme: Theme,
    kind: WallpaperKind,
    selected: bool,
    width: f32,
    thumbnail: Div,
    badge_id: usize,
    badge_selector: String,
    delete: Option<(usize, String, bool)>,
}

fn thumbnail_box(content: ThumbnailBoxContent, cx: &mut Context<MultisoulApp>) -> Stateful<Div> {
    let ThumbnailBoxContent {
        theme,
        kind,
        selected,
        width,
        thumbnail,
        badge_id,
        badge_selector,
        delete,
    } = content;
    let border = if selected {
        with_alpha(theme.accent_blue, 0xff)
    } else {
        with_alpha(theme.edge_highlight, 0x40)
    };

    let mut frame = div()
        .w(px(width - 10.0))
        .h(px((width - 10.0) * 9.0 / 16.0))
        .relative()
        .overflow_hidden()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(border))
        .bg(rgba(with_alpha(theme.panel_high, 0x80)))
        .id(("wallpaper-preview", badge_id))
        .debug_selector(move || format!("settings-wallpaper-preview-{badge_id}"))
        .child(thumbnail)
        .when(kind != WallpaperKind::Static, |frame| {
            frame.child(wallpaper_badge(theme, kind, badge_id, badge_selector))
        })
        .when(selected, |frame| {
            frame.child(
                div()
                    .absolute()
                    .right(px(7.0))
                    .bottom(px(7.0))
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

    if let Some((idx, filename, busy)) = delete {
        frame = frame.child(delete_button(theme, idx, filename, busy, cx));
    }
    frame
}

/// Compact, material-like media badge inspired by macOS thumbnail overlays.
/// A neutral capsule keeps the label legible over any image; only the dot uses
/// semantic color so a grid of wallpapers stays calm and easy to scan.
fn wallpaper_badge(
    theme: Theme,
    kind: WallpaperKind,
    badge_id: usize,
    selector: String,
) -> Stateful<Div> {
    let is_dynamic = matches!(kind, WallpaperKind::Gif | WallpaperKind::Video);
    div()
        .id(("wallpaper-kind-badge", badge_id))
        .debug_selector(move || selector.clone())
        .absolute()
        .left(px(7.0))
        .bottom(px(7.0))
        .h(px(20.0))
        .px(px(7.0))
        .flex()
        .items_center()
        .gap(px(5.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(0xffffff38))
        .bg(rgba(0x151515c2))
        .settings_font(TextRole::Badge)
        .text_color(rgba(0xffffffff))
        .child(
            div()
                .w(px(5.0))
                .h(px(5.0))
                .flex_none()
                .rounded(px(2.5))
                .bg(rgba(if is_dynamic {
                    with_alpha(theme.accent_cyan, 0xff)
                } else {
                    0xffffff80
                })),
        )
        .child(ui::i18n::text(if is_dynamic {
            "settings.wallpaper_dynamic"
        } else {
            "settings.wallpaper_static"
        }))
}

/// Hover-only delete affordance for a custom cell. Stops propagation so the
/// click does not also select the underlying cell.
fn delete_button(
    theme: Theme,
    idx: usize,
    filename: String,
    busy: bool,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    div()
        .id(("wallpaper-delete-container", idx))
        .absolute()
        .top(px(4.0))
        .right(px(4.0))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            Button::new(("wallpaper-delete", idx))
                .ghost()
                .label("×")
                .tooltip(ui::i18n::text("settings.delete_shared_library"))
                .disabled(busy)
                .w(px(24.0))
                .h(px(24.0))
                .text_color(rgba(theme.text_primary))
                .on_wallpaper_activate(
                    !busy,
                    cx.listener(move |app, _, window, cx| {
                        cx.stop_propagation();
                        app.request_wallpaper_delete(filename.clone(), window, cx);
                    }),
                ),
        )
}

/// The label row under a cell: highlighted when selected.
fn thumbnail_label(theme: Theme, text: &str, selected: bool) -> Div {
    div()
        .settings_font(TextRole::Label)
        .text_color(rgba(if selected {
            theme.text_primary
        } else {
            theme.text_secondary
        }))
        .truncate()
        .child(text.to_string())
}

/// Render every media kind through the same bounded, single-frame cache.
/// This avoids GPUI's retain-all asset cache and prevents animated GIF frames
/// or full-resolution video stills from entering the Metal atlas.
pub(super) fn preview_thumbnail(
    kind: WallpaperKind,
    path: Option<PathBuf>,
    previews: &mut PreviewCache,
    radius: f32,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let Some(path) = path else {
        return preview_placeholder(kind);
    };
    let kind = if path
        .extension()
        .is_some_and(|extension| extension == "webp")
    {
        WallpaperKind::Static
    } else {
        kind
    };
    if let Some(image) = previews.get(&path) {
        return div().size_full().child(
            img(ImageSource::Render(image))
                .size_full()
                .rounded(px(radius))
                .object_fit(ObjectFit::Cover),
        );
    }
    super::preview::request(kind, path, previews, cx);
    preview_placeholder(kind)
}

fn preview_placeholder(kind: WallpaperKind) -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(match kind {
            WallpaperKind::Static => ui::i18n::text("settings.wallpaper_image"),
            WallpaperKind::Gif => "GIF",
            WallpaperKind::Video => ui::i18n::text("settings.wallpaper_video"),
        })
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
