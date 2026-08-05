//! GPUI view for the Wallpaper settings section.
//!
//! Renders a thumbnail grid (builtins + custom) with an Import button. Each
//! cell is clickable to select that wallpaper; custom cells additionally show
//! a hover delete affordance. Video wallpapers render the decoded first-frame
//! thumbnail (Task 11) once the background AVAssetReader decode completes,
//! falling back to a labeled placeholder while decoding or on non-mac builds.
//!
//! State lives in [`super::state`]; this module only renders against
//! `&WallpaperSettingsState` and routes clicks back into `MultisoulApp`.

use super::dirs::custom_wallpapers_dir;
use super::preview::PreviewCache;
use super::state::WallpaperSettingsState;
use crate::app::wallpaper_resolvers::{builtin_wallpapers, resolve_builtin_asset, WallpaperKind};
use crate::app::MultisoulApp;
use gpui::{
    div, img, prelude::*, px, rgba, ClickEvent, Context, Div, FontWeight, ImageSource, MouseButton,
    ObjectFit, Stateful,
};
use std::path::PathBuf;
use ui::Theme;

const THUMB_W: f32 = 168.0;
const THUMB_H: f32 = 104.0;
const GRID_GAP: f32 = 16.0;

struct BuiltinWallpaperCell {
    idx: usize,
    rel: &'static str,
    name: &'static str,
    kind: WallpaperKind,
}

/// Render the Wallpaper settings section body.
pub(crate) fn wallpaper_settings(
    theme: Theme,
    state: &WallpaperSettingsState,
    previews: &mut PreviewCache,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let builtins = builtin_wallpapers();
    let customs = state.list_custom();

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .pt(px(8.0))
        .child(import_row(theme, cx))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(GRID_GAP))
                .children(
                    builtins
                        .iter()
                        .enumerate()
                        .map(|(idx, &(rel, name, kind))| {
                            builtin_cell(
                                theme,
                                state,
                                previews,
                                BuiltinWallpaperCell {
                                    idx,
                                    rel,
                                    name,
                                    kind,
                                },
                                cx,
                            )
                        }),
                )
                .children(
                    customs.iter().enumerate().map(|(idx, filename)| {
                        custom_cell(theme, state, previews, idx, filename, cx)
                    }),
                ),
        )
}

/// The Import button row. Clicking it spawns the rfd file picker (Task 8):
/// `MultisoulApp::import_wallpaper_file` runs the picker off the main thread
/// and copies the chosen file into the custom wallpapers dir.
fn import_row(theme: Theme, cx: &mut Context<MultisoulApp>) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .id("wallpaper-import")
                .px(px(16.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.accent_cyan, 0x50)))
                .bg(rgba(with_alpha(theme.accent_cyan, 0x12)))
                .text_size(px(14.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(theme.text_primary))
                .cursor_pointer()
                .hover(move |style| style.bg(rgba(with_alpha(theme.accent_cyan, 0x22))))
                .on_click(cx.listener(|app, _: &ClickEvent, _window, cx| {
                    app.import_wallpaper_file(cx);
                }))
                .child("Import wallpaper…"),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_tertiary))
                .child("PNG / JPG / GIF / MP4 / MOV"),
        )
}

/// One grid cell for a builtin wallpaper.
fn builtin_cell(
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
    } = cell;
    let selected = state.is_selected_builtin(rel);
    div()
        .id(("wallpaper-thumb", idx))
        .w(px(THUMB_W))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .cursor_pointer()
        .on_click(cx.listener(move |app, _: &ClickEvent, _window, cx| {
            app.wallpaper_settings.select_builtin(rel);
            cx.notify();
        }))
        .child(thumbnail_box(
            ThumbnailBoxContent {
                theme,
                kind,
                selected,
                thumbnail: preview_thumbnail(kind, resolve_builtin_asset(rel), previews, cx),
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
}

/// One grid cell for a custom wallpaper. Custom cells expose a hover-only
/// delete button (id `("wallpaper-delete", idx)`); builtins never do.
fn custom_cell(
    theme: Theme,
    state: &WallpaperSettingsState,
    previews: &mut PreviewCache,
    idx: usize,
    filename: &str,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let selected = state.is_selected_custom(filename);
    let filename_owned = filename.to_string();
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
    let thumb = preview_thumbnail(kind, path, previews, cx);

    div()
        .id(("wallpaper-thumb-custom", idx))
        .w(px(THUMB_W))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .cursor_pointer()
        .on_click(cx.listener(move |app, _: &ClickEvent, _window, cx| {
            app.wallpaper_settings.select_custom(filename_owned.clone());
            cx.notify();
        }))
        .child(thumbnail_box(
            ThumbnailBoxContent {
                theme,
                kind,
                selected,
                thumbnail: thumb,
                badge_id: builtin_wallpapers().len() + idx,
                badge_selector: format!(
                    "settings-wallpaper-custom-badge-{idx}-{}",
                    kind.badge_label().to_ascii_lowercase()
                ),
                delete: Some((idx, delete_filename)),
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
    thumbnail: Div,
    badge_id: usize,
    badge_selector: String,
    delete: Option<(usize, String)>,
}

fn thumbnail_box(content: ThumbnailBoxContent, cx: &mut Context<MultisoulApp>) -> Stateful<Div> {
    let ThumbnailBoxContent {
        theme,
        kind,
        selected,
        thumbnail,
        badge_id,
        badge_selector,
        delete,
    } = content;
    let border = if selected {
        with_alpha(theme.accent_orange, 0xff)
    } else {
        with_alpha(theme.edge_highlight, 0x40)
    };

    let mut frame = div()
        .w(px(THUMB_W))
        .h(px(THUMB_H))
        .relative()
        .overflow_hidden()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(border))
        .bg(rgba(with_alpha(theme.panel_high, 0x80)))
        .id(("wallpaper-preview", badge_id))
        .debug_selector(move || format!("settings-wallpaper-preview-{badge_id}"))
        .child(thumbnail)
        .child(wallpaper_badge(theme, kind, badge_id, badge_selector));

    if let Some((idx, filename)) = delete {
        frame = frame.child(delete_button(theme, idx, filename, cx));
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
        .text_size(px(10.0))
        .line_height(px(12.0))
        .font_weight(FontWeight::MEDIUM)
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
        .child(kind.badge_label())
}

/// Hover-only delete affordance for a custom cell. Stops propagation so the
/// click does not also select the underlying cell.
fn delete_button(
    theme: Theme,
    idx: usize,
    filename: String,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    div()
        .id(("wallpaper-delete", idx))
        .absolute()
        .top(px(4.0))
        .right(px(4.0))
        .w(px(20.0))
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.0))
        .text_size(px(12.0))
        .line_height(px(14.0))
        .text_color(rgba(theme.text_primary))
        .bg(rgba(with_alpha(theme.destructive, 0x00)))
        .hover(move |style| style.bg(rgba(with_alpha(theme.destructive, 0xc0))))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |app, _: &ClickEvent, _window, cx| {
            app.wallpaper_settings.delete_custom(&filename);
            cx.notify();
        }))
        .child("x")
}

/// The label row under a cell: highlighted when selected.
fn thumbnail_label(theme: Theme, text: &str, selected: bool) -> Div {
    div()
        .text_size(px(13.0))
        .font_weight(if selected {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        })
        .text_color(rgba(if selected {
            theme.text_primary
        } else {
            theme.text_secondary
        }))
        .child(text.to_string())
}

/// Render every media kind through the same bounded, single-frame cache.
/// This avoids GPUI's retain-all asset cache and prevents animated GIF frames
/// or full-resolution video stills from entering the Metal atlas.
fn preview_thumbnail(
    kind: WallpaperKind,
    path: Option<PathBuf>,
    previews: &mut PreviewCache,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let Some(path) = path else {
        return preview_placeholder(kind);
    };
    if let Some(image) = previews.get(&path) {
        return div().size_full().child(
            img(ImageSource::Render(image))
                .size_full()
                .rounded(px(7.0))
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
            WallpaperKind::Static => "IMAGE",
            WallpaperKind::Gif => "GIF",
            WallpaperKind::Video => "VIDEO",
        })
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
