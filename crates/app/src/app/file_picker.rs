//! Native file picker for wallpaper import (Task 8).
//!
//! Wraps `rfd::AsyncFileDialog` with the wallpaper file filter. The async
//! variant is preferred over `rfd::FileDialog::pick_file` (blocking) because
//! on macOS the native panel must be driven from the main thread and the
//! awaitable future resolves safely without blocking the GPUI event loop.
//!
//! The returned future can be awaited directly inside a `cx.spawn` task — it
//! does not need a background executor because `AsyncFileDialog` itself
//! marshals onto the platform's main run loop.

use std::path::PathBuf;

/// Accepted wallpaper extensions, shared by the filter and import validation.
pub(crate) const WALLPAPER_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "mp4", "mov"];
pub(crate) const ZIPVOICE_REFERENCE_EXTENSIONS: &[&str] = &["wav"];

/// Open a native "Choose a wallpaper" panel and await the user's selection.
///
/// Returns `None` when the user cancels. Returns `Some(path)` carrying the
/// absolute picked path otherwise. The filter restricts the visible files to
/// [`WALLPAPER_EXTENSIONS`], but the picker does not guarantee the chosen
/// extension — `WallpaperSettingsState::import_custom` re-validates the
/// extension before copying.
pub(crate) async fn pick_wallpaper_image() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Choose a wallpaper")
        .add_filter("Images & Video", WALLPAPER_EXTENSIONS)
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}

pub(crate) async fn pick_zipvoice_reference_audio() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Choose a ZipVoice reference WAV")
        .add_filter("16 kHz mono PCM WAV", ZIPVOICE_REFERENCE_EXTENSIONS)
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}
