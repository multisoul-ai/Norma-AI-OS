//! GPUI view for the Wallpaper settings section.
//!
//! Renders a thumbnail grid (builtins + custom) with an Import button. Each
//! cell is clickable to select that wallpaper; custom cells additionally show
//! a hover delete affordance. Video wallpapers render a labeled placeholder
//! for now — Task 11 replaces it with the first-frame thumbnail.
//!
//! State lives in [`super::state`]; this module only renders against
//! `&WallpaperSettingsState` and routes clicks back into `MultisoulApp`.

use super::state::{custom_wallpapers_dir, WallpaperSettingsState};
use crate::app::wallpaper_resolvers::{builtin_wallpapers, WallpaperKind};
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

/// Render the Wallpaper settings section body.
pub(crate) fn wallpaper_settings(
    theme: Theme,
    state: &WallpaperSettingsState,
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
                            builtin_cell(theme, state, idx, rel, name, kind, cx)
                        }),
                )
                .children(
                    customs
                        .iter()
                        .enumerate()
                        .map(|(idx, filename)| custom_cell(theme, state, idx, filename, cx)),
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
    idx: usize,
    rel: &'static str,
    name: &'static str,
    kind: WallpaperKind,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
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
            theme,
            kind,
            selected,
            builtin_thumbnail(kind, rel),
            None,
            cx,
        ))
        .child(thumbnail_label(theme, name, selected))
}

/// One grid cell for a custom wallpaper. Custom cells expose a hover-only
/// delete button (id `("wallpaper-delete", idx)`); builtins never do.
fn custom_cell(
    theme: Theme,
    state: &WallpaperSettingsState,
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
    let thumb = custom_thumbnail(kind, path);

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
            theme,
            kind,
            selected,
            thumb,
            Some((idx, delete_filename)),
            cx,
        ))
        .child(thumbnail_label(theme, filename, selected))
}

/// The fixed-size thumbnail frame. `delete` carries `(idx, filename)` for
/// custom cells so a hover delete affordance can be layered on top; `None`
/// for builtins (they are never deletable).
fn thumbnail_box(
    theme: Theme,
    _kind: WallpaperKind,
    selected: bool,
    thumb: Div,
    delete: Option<(usize, String)>,
    cx: &mut Context<MultisoulApp>,
) -> Div {
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
        .child(thumb);

    if let Some((idx, filename)) = delete {
        frame = frame.child(delete_button(theme, idx, filename, cx));
    }
    frame
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

/// Builtin thumbnail content. Static/Gif render from the embedded asset path
/// (ImageSource::from(&'static str)); Video renders a placeholder until Task
/// 11 ships the first-frame thumbnail.
fn builtin_thumbnail(kind: WallpaperKind, rel: &'static str) -> Div {
    match kind {
        WallpaperKind::Static | WallpaperKind::Gif => div().size_full().child(
            img(ImageSource::from(rel))
                .size_full()
                .object_fit(ObjectFit::Cover),
        ),
        WallpaperKind::Video => video_placeholder(),
    }
}

/// Custom thumbnail content. Static/Gif render from the on-disk path
/// (ImageSource::from(PathBuf)); Video renders a placeholder until Task 11.
fn custom_thumbnail(kind: WallpaperKind, path: Option<PathBuf>) -> Div {
    match kind {
        WallpaperKind::Static | WallpaperKind::Gif => match path {
            Some(p) => div().size_full().child(
                img(ImageSource::from(p))
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            ),
            None => div().size_full(),
        },
        WallpaperKind::Video => video_placeholder(),
    }
}

// Task 11: video first-frame thumbnail. Until then the grid labels a video
// cell with "VIDEO" so the affordance is discoverable without a live decoder.
fn video_placeholder() -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child("VIDEO")
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
