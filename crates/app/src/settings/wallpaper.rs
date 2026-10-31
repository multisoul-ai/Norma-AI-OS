//! Wallpaper settings: state/persistence + GPUI view.
//!
//! Mirrors the `voice` split: `state.rs` holds [`WallpaperSettingsState`] and
//! the import flow; `view.rs` holds the settings-panel grid. Tests covering
//! both live in `crates/app/tests/unit/wallpaper_settings_tests.rs`.

mod action;
mod cells;
mod current;
pub(crate) mod delete_dialog;
pub(crate) mod dirs;
mod import;
pub(crate) mod layout;
mod migration;
pub(crate) mod preferences;
pub(crate) mod preview;
pub(crate) mod state;
mod transition;
mod view;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_layout_tests.rs"]
mod layout_tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_scope_tests.rs"]
mod scope_tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_actions_tests.rs"]
mod actions_tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_keyboard_tests.rs"]
mod keyboard_tests;

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
#[path = "../../tests/unit/wallpaper_transition_tests.rs"]
mod transition_tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_builtin_migration_tests.rs"]
mod builtin_migration_tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_dirs_tests.rs"]
mod dirs_tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_optional_choice_tests.rs"]
mod optional_choice_tests;

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_preview_tests.rs"]
mod preview_tests;
