//! Wallpaper settings: state/persistence + GPUI view.
//!
//! Mirrors the `voice` split: `state.rs` holds [`WallpaperSettingsState`] and
//! the import flow; `view.rs` holds the settings-panel grid. Tests covering
//! both live in `crates/app/tests/unit/wallpaper_settings_tests.rs`.

mod dirs;
mod import;
pub(crate) mod preview;
mod state;
mod view;

pub(crate) use import::ImportResult;
pub(crate) use preview::PreviewCache;
pub(crate) use state::WallpaperSettingsState;
pub(crate) use view::wallpaper_settings;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_settings_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_render_id_tests.rs"]
mod render_id_tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_dirs_tests.rs"]
mod dirs_tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_preview_tests.rs"]
mod preview_tests;
