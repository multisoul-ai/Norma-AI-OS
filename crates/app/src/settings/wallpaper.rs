//! Wallpaper settings state: persistent wallpaper selection via SettingsStore.
//!
//! Mirrors [`crate::settings::voice::VoiceSettingsState`] in structure:
//! - `KEY_*` constants hold storage row keys.
//! - `load_default()` opens the default app store (or, when
//!   `MULTISOUL_TEST_DATA_ROOT` is set, an app.sqlite under that root) and
//!   reconstructs the selection from the saved keys.
//! - mutating methods update the in-memory selection and write through to the
//!   SettingsStore; the caller (Task 5) is responsible for invoking
//!   `cx.notify()` afterwards to refresh the rendered view.
//!
//! This module contains state + persistence + path resolution + custom dir
//! listing only. The GPUI settings view is split into `wallpaper_view.rs` in
//! Task 7.

use std::path::{Path, PathBuf};

use crate::app::wallpaper_resolvers::{builtin_wallpapers, resolve_builtin_asset, WallpaperKind};
use crate::assets::DEFAULT_WALLPAPER_ASSET;
use storage::{AppStore, SettingsStore};

/// Settings row key holding the selection kind: `"builtin"` or `"custom"`.
const KEY_WALLPAPER_KIND: &str = "wallpaper.kind";
/// Settings row key holding the selection value: builtin rel path
/// (e.g. `"wallpapers/1331865.png"`) or custom filename (e.g. `"mine.png"`).
const KEY_WALLPAPER_VALUE: &str = "wallpaper.value";

const KIND_BUILTIN: &str = "builtin";
const KIND_CUSTOM: &str = "custom";

/// Env var overriding the data dir root for tests. When set, both the app db
/// (`<root>/app.sqlite`) and the custom wallpapers dir (`<root>/wallpapers`)
/// are routed underneath it. Production callers do not set this var and fall
/// back to `default_app_db_path()?.parent()`.
const TEST_DATA_ROOT_ENV: &str = "MULTISOUL_TEST_DATA_ROOT";

/// The user's wallpaper selection.
///
/// `Builtin` holds a `&'static str` because the set of builtins is fixed at
/// compile time and the rel path must be one of the entries in
/// [`builtin_wallpapers`]. `Custom` holds an owned `String` filename (basename
/// only — the file lives under the custom wallpapers dir).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WallpaperSelection {
    /// 内置壁纸：相对资源路径（必须出现在 `builtin_wallpapers()` 清单中）。
    Builtin { rel: &'static str },
    /// 自定义壁纸：仅文件名（basename），位于数据目录 `wallpapers/` 下。
    Custom { filename: String },
}

/// Persistent wallpaper selection state.
///
/// `selection` is the authoritative in-memory state. `storage` is the
/// SettingsStore (None only if the db could not be opened, in which case
/// persistence becomes a no-op).
#[derive(Clone, Debug)]
pub(crate) struct WallpaperSettingsState {
    pub(crate) selection: WallpaperSelection,
    storage: Option<SettingsStore>,
}

impl WallpaperSettingsState {
    /// Load the saved wallpaper selection from the default data store.
    ///
    /// Honors `MULTISOUL_TEST_DATA_ROOT` so that tests run against an isolated
    /// `<root>/app.sqlite` instead of the real user db. When the saved
    /// selection cannot be resolved (unknown rel / missing file / corrupt
    /// kind), it falls back to [`DEFAULT_WALLPAPER_ASSET`] and persists the
    /// correction so subsequent loads are clean.
    pub(crate) fn load_default() -> Self {
        Self::load_from(open_data_store())
    }

    /// Test-only entry: opens the store under `MULTISOUL_TEST_DATA_ROOT`.
    ///
    /// Identical to [`load_default`]; kept as a separate symbol so test code
    /// documents intent and production callers don't accidentally invoke it.
    #[cfg(test)]
    pub(crate) fn load_default_test() -> Self {
        Self::load_from(open_data_store())
    }

    /// Test-only entry: opens the store under `MULTISOUL_TEST_DATA_ROOT` and
    /// does not assume any prior saved state. Equivalent to
    /// [`load_default_test`]; provided so the brief's T-W1 (which expects no
    /// preference to exist) reads as documented.
    #[cfg(test)]
    pub(crate) fn load_default_with_test_store() -> Self {
        Self::load_from(open_data_store())
    }

    /// Reconstruct state from an optional SettingsStore. When `store` is None
    /// (db unreachable) the default selection is used in memory only.
    fn load_from(store: Option<SettingsStore>) -> Self {
        let saved = store
            .as_ref()
            .and_then(|s| load_saved_selection(s).ok().flatten());

        match saved {
            Some(selection) if selection_resolves(&selection) => Self {
                selection,
                storage: store,
            },
            Some(bad) => {
                tracing::warn!(
                    ?bad,
                    "saved wallpaper selection did not resolve; falling back to default"
                );
                let mut state = Self::default_with(store);
                state.persist();
                state
            }
            None => Self::default_with(store),
        }
    }

    fn default_with(storage: Option<SettingsStore>) -> Self {
        Self {
            selection: WallpaperSelection::Builtin {
                rel: DEFAULT_WALLPAPER_ASSET,
            },
            storage,
        }
    }

    /// Select a builtin wallpaper by its static rel path and persist.
    /// Caller must invoke `cx.notify()` to refresh the view.
    pub(crate) fn select_builtin(&mut self, rel: &'static str) {
        self.selection = WallpaperSelection::Builtin { rel };
        self.persist();
    }

    /// Select a custom wallpaper by filename (basename under the custom dir)
    /// and persist. Caller must invoke `cx.notify()` to refresh the view.
    pub(crate) fn select_custom(&mut self, filename: String) {
        self.selection = WallpaperSelection::Custom { filename };
        self.persist();
    }

    /// Classify the current selection's kind. For an unknown builtin rel the
    /// kind defaults to Static; an unknown custom extension also defaults to
    /// Static.
    pub(crate) fn current_kind(&self) -> WallpaperKind {
        selection_kind(&self.selection)
    }

    /// Resolve the selection to a runtime-readable absolute path.
    ///
    /// Builtin entries resolve via [`resolve_builtin_asset`]; custom entries
    /// resolve to `<data_root>/wallpapers/<filename>` only when the file
    /// exists. Returns None when resolution fails — callers should fall back
    /// to the default wallpaper.
    pub(crate) fn resolve_path(&self) -> Option<PathBuf> {
        match &self.selection {
            WallpaperSelection::Builtin { rel } => resolve_builtin_asset(rel),
            WallpaperSelection::Custom { filename } => {
                let dir = custom_wallpapers_dir()?;
                let path = dir.join(filename);
                if path.is_file() {
                    Some(path)
                } else {
                    None
                }
            }
        }
    }

    /// Return the asset path for GPUI `ImageSource::Resource` rendering.
    ///
    /// Only builtin image wallpapers (Static / Gif) opt into asset-path
    /// rendering — they ship inside the binary via `EmbeddedAssets`. Videos
    /// bypass this (Task 10 territory) and custom files render via
    /// `ImageSource::from(PathBuf)` instead, so they both return None here.
    pub(crate) fn image_asset_path(&self) -> Option<&'static str> {
        match &self.selection {
            WallpaperSelection::Builtin { rel } => match selection_kind(&self.selection) {
                WallpaperKind::Static | WallpaperKind::Gif => Some(*rel),
                WallpaperKind::Video => None,
            },
            WallpaperSelection::Custom { .. } => None,
        }
    }

    /// List custom wallpaper filenames (basenames) currently in the custom
    /// wallpapers dir. Filters by [`WallpaperKind::from_extension`]; unknown
    /// extensions are excluded. Sorted lexicographically for stable UI order.
    pub(crate) fn list_custom(&self) -> Vec<String> {
        let Some(dir) = custom_wallpapers_dir() else {
            return Vec::new();
        };
        let Ok(read) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out: Vec<String> = read
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if !path.is_file() {
                    return None;
                }
                let ext = path.extension().and_then(|e| e.to_str())?;
                if WallpaperKind::from_extension(ext).is_none() {
                    return None;
                }
                path.file_name().and_then(|n| n.to_str()).map(str::to_owned)
            })
            .collect();
        out.sort();
        out
    }

    /// Delete a custom wallpaper file from disk. If the deleted file was the
    /// current selection, reset to the default builtin and persist the change.
    /// Caller must invoke `cx.notify()` to refresh the view.
    pub(crate) fn delete_custom(&mut self, filename: &str) {
        if let Some(dir) = custom_wallpapers_dir() {
            let path = dir.join(filename);
            if path.is_file() {
                if let Err(e) = std::fs::remove_file(&path) {
                    tracing::warn!(?path, error = %e, "failed to remove custom wallpaper");
                }
            }
        }
        let is_current =
            matches!(&self.selection, WallpaperSelection::Custom { filename: f } if f == filename);
        if is_current {
            self.selection = WallpaperSelection::Builtin {
                rel: DEFAULT_WALLPAPER_ASSET,
            };
            self.persist();
        }
    }

    /// Write the current selection to the SettingsStore. No-op when the store
    /// is unavailable. Failures are logged but do not panic.
    fn persist(&mut self) {
        let Some(store) = self.storage.as_ref() else {
            return;
        };
        let (kind, value) = match &self.selection {
            WallpaperSelection::Builtin { rel } => (KIND_BUILTIN, (*rel).to_string()),
            WallpaperSelection::Custom { filename } => (KIND_CUSTOM, filename.clone()),
        };
        if let Err(e) = store.set_string(KEY_WALLPAPER_KIND, kind) {
            tracing::warn!(error = %e, "failed to persist wallpaper kind");
        }
        if let Err(e) = store.set_string(KEY_WALLPAPER_VALUE, &value) {
            tracing::warn!(error = %e, "failed to persist wallpaper value");
        }
    }
}

/// Read both keys and reconstruct a selection. Returns None when either
/// key is missing, the kind is unknown, the builtin rel is not in the
/// registry, or the custom filename is malformed (empty / contains a path
/// separator). The caller decides whether to fall back to default.
fn load_saved_selection(
    store: &SettingsStore,
) -> storage::StorageResult<Option<WallpaperSelection>> {
    let kind: Option<String> = store.get_string(KEY_WALLPAPER_KIND)?;
    let value: Option<String> = store.get_string(KEY_WALLPAPER_VALUE)?;
    let (Some(kind), Some(value)) = (kind, value) else {
        return Ok(None);
    };
    let value_str = value.as_str();
    let selection = match kind.as_str() {
        KIND_BUILTIN => {
            let rel: Option<&'static str> = builtin_wallpapers()
                .iter()
                .find(|(r, _, _)| *r == value_str)
                .map(|(r, _, _)| *r);
            match rel {
                Some(r) => WallpaperSelection::Builtin { rel: r },
                None => return Ok(None),
            }
        }
        KIND_CUSTOM => {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains('\\') {
                return Ok(None);
            }
            WallpaperSelection::Custom {
                filename: trimmed.to_string(),
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(selection))
}

/// Classify an arbitrary selection's WallpaperKind. Unknown builtin rel
/// (shouldn't happen — builtins come from the registry) defaults to Static.
/// Unknown custom extension also defaults to Static rather than being
/// unrenderable, matching the strictest-but-safe behavior.
fn selection_kind(sel: &WallpaperSelection) -> WallpaperKind {
    match sel {
        WallpaperSelection::Builtin { rel } => builtin_wallpapers()
            .iter()
            .find(|(r, _, _)| *r == *rel)
            .map(|(_, _, k)| *k)
            .unwrap_or(WallpaperKind::Static),
        WallpaperSelection::Custom { filename } => {
            let ext = Path::new(filename)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            WallpaperKind::from_extension(ext).unwrap_or(WallpaperKind::Static)
        }
    }
}

/// Whether the selection can be resolved to a real on-disk asset right now.
/// Builtin entries need [`resolve_builtin_asset`] to succeed; custom entries
/// need the file present in the custom dir. Used at load time to discard
/// stale persisted selections (e.g. user deleted a custom file out of band).
fn selection_resolves(sel: &WallpaperSelection) -> bool {
    match sel {
        WallpaperSelection::Builtin { rel } => resolve_builtin_asset(rel).is_some(),
        WallpaperSelection::Custom { filename } => custom_wallpapers_dir()
            .map(|d| d.join(filename).is_file())
            .unwrap_or(false),
    }
}

/// Absolute path to the custom wallpapers directory.
///
/// Production: `~/Library/Application Support/Multisoul/wallpapers/`.
/// Tests: `<MULTISOUL_TEST_DATA_ROOT>/wallpapers/`. Returns None when the
/// data root cannot be computed (rare: missing `$HOME` both in prod and when
/// env var unset).
pub(crate) fn custom_wallpapers_dir() -> Option<PathBuf> {
    data_root().map(|p| p.join("wallpapers"))
}

/// Resolve the data dir root. Honors `MULTISOUL_TEST_DATA_ROOT` when set;
/// otherwise falls back to the parent of the default app db path.
fn data_root() -> Option<PathBuf> {
    if let Some(test_root) = std::env::var(TEST_DATA_ROOT_ENV)
        .ok()
        .filter(|s| !s.is_empty())
    {
        return Some(PathBuf::from(test_root));
    }
    storage::default_app_db_path()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

/// Open the SettingsStore used by [`WallpaperSettingsState::load_default`].
///
/// When `MULTISOUL_TEST_DATA_ROOT` is set, opens `<root>/app.sqlite` (the
/// file may not yet exist — the first `set_string` call runs the migrations
/// that create it). Otherwise opens the production default app store.
fn open_data_store() -> Option<SettingsStore> {
    if std::env::var(TEST_DATA_ROOT_ENV).is_ok() {
        // data_root() honors the same env var, so this is the test root.
        let root = data_root().expect(
            "MULTISOUL_TEST_DATA_ROOT is set but data_root() returned None — \
             env var resolution is bugged",
        );
        return Some(AppStore::new(root.join("app.sqlite")).settings_store());
    }
    AppStore::open_default()
        .ok()
        .map(|store| store.settings_store())
}

#[cfg(test)]
#[path = "../../tests/unit/wallpaper_settings_tests.rs"]
mod tests;
