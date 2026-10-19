//! Wallpaper settings state: persistent wallpaper selection via SettingsStore.
//!
//! Mirrors [`crate::settings::voice::VoiceSettingsState`] in structure:
//! - `KEY_*` constants hold storage row keys.
//! - `load_from_store()` reconstructs the selection from the App composition
//!   root's already-selected SettingsStore.
//! - mutating methods update the in-memory selection and write through to the
//!   SettingsStore; the caller (Task 5) is responsible for invoking
//!   `cx.notify()` afterwards to refresh the rendered view.
//!
//! This module contains state + persistence + path resolution + custom dir
//! listing + the import flow. The GPUI settings view lives in `view.rs`.

use std::path::{Path, PathBuf};

use super::dirs::custom_wallpapers_dir;
use super::import::{non_colliding_dest, ImportResult};
use crate::app::wallpaper_resolvers::{
    builtin_wallpapers, canonical_builtin_wallpaper, resolve_builtin_asset, WallpaperKind,
};
use crate::assets::DEFAULT_WALLPAPER_ASSET;
use storage::SettingsStore;

/// Settings row key holding the selection kind: `"builtin"` or `"custom"`.
const KEY_WALLPAPER_KIND: &str = "wallpaper.kind";
/// Settings row key holding the selection value: builtin rel path
/// (e.g. `"wallpapers/1331865.webp"`) or custom filename (e.g. `"mine.png"`).
const KEY_WALLPAPER_VALUE: &str = "wallpaper.value";

const KIND_BUILTIN: &str = "builtin";
const KIND_CUSTOM: &str = "custom";

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
    pub(super) transition: Option<super::transition::WallpaperTransition>,
    storage: Option<SettingsStore>,
    pub(crate) scoped: super::scoped::ScopedWallpaperState,
}

impl WallpaperSettingsState {
    pub(crate) fn load_from_store(store: SettingsStore) -> Self {
        let mut state = Self::load_from(Some(store));
        state.scoped.customs = state.list_custom();
        state.scoped.global = state.selection.clone();
        state
    }

    /// Reconstruct state from an optional SettingsStore. When `store` is None
    /// (db unreachable) the default selection is used in memory only.
    fn load_from(store: Option<SettingsStore>) -> Self {
        let saved = store
            .as_ref()
            .and_then(|s| load_saved_selection(s).ok().flatten());

        match saved {
            Some(loaded) if selection_resolves(&loaded.selection) => {
                let mut state = Self {
                    scoped: super::scoped::ScopedWallpaperState::new(
                        loaded.selection.clone(),
                        Vec::new(),
                    ),
                    selection: loaded.selection,
                    transition: None,
                    storage: store,
                };
                if loaded.needs_persist {
                    state.persist();
                }
                state
            }
            Some(bad) => {
                tracing::warn!(
                    selection = ?bad.selection,
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
            transition: None,
            storage,
            scoped: super::scoped::ScopedWallpaperState::new(
                WallpaperSelection::default(),
                Vec::new(),
            ),
        }
    }

    /// Select a builtin wallpaper by its static rel path and persist.
    /// Caller must invoke `cx.notify()` to refresh the view.
    #[cfg(test)]
    pub(crate) fn select_builtin(&mut self, rel: &'static str) {
        self.selection = WallpaperSelection::Builtin { rel };
        self.persist();
    }

    /// Select a custom wallpaper by filename (basename under the custom dir)
    /// and persist. Caller must invoke `cx.notify()` to refresh the view.
    #[cfg(test)]
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

    /// A stable key identifying the *currently rendered* wallpaper source.
    ///
    /// Used as the GPUI `img()` element id suffix so the element identity
    /// changes when the wallpaper source changes. This is REQUIRED: GPUI 0.2.2
    /// keeps `ImgState.frame_index` per-element-id across renders, and a
    /// multi-frame GIF can advance `frame_index` past 1. If the user then
    /// switches to a single-frame static image under the SAME id, the stale
    /// `frame_index` indexes the new 1-frame `RenderImage.data` →
    /// `index out of bounds` panic (gpui `assets.rs:79`). A source-derived id
    /// forces GPUI to discard the old state and reset `frame_index` to 0.
    ///
    /// The key covers all render paths: builtins (incl. default fallback) and
    /// customs; switching kind (e.g. video↔image) also changes the key.
    pub(crate) fn render_id_key(&self) -> String {
        self.selection.render_id_key()
    }

    /// Whether the given builtin rel path is the currently selected wallpaper.
    /// Used by the grid view to render the highlight border without poking the
    /// `selection` enum directly.
    pub(crate) fn is_selected_builtin(&self, rel: &str) -> bool {
        matches!(&self.selection, WallpaperSelection::Builtin { rel: r } if *r == rel)
    }

    /// Whether the given custom filename is the currently selected wallpaper.
    /// Used by the grid view to render the highlight border without poking the
    /// `selection` enum directly.
    pub(crate) fn is_selected_custom(&self, filename: &str) -> bool {
        matches!(&self.selection, WallpaperSelection::Custom { filename: f } if f == filename)
    }

    /// Resolve the selection to a runtime-readable absolute path.
    ///
    /// Builtin entries resolve via [`resolve_builtin_asset`]; custom entries
    /// resolve to `<data_root>/wallpapers/<filename>` only when the file
    /// exists. Returns None when resolution fails — callers should fall back
    /// to the default wallpaper.
    pub(crate) fn resolve_path(&self) -> Option<PathBuf> {
        resolve_selection_path(&self.selection)
    }

    /// Build the GPUI [`ImageSource`] the chrome wallpaper layer should render.
    ///
    /// This is the single source of truth used by the chrome backdrop so the
    /// render decision lives next to the persistent state instead of being
    /// re-derived at the view layer:
    ///
    /// - Builtin Static/Gif → resolve the packaged resource to an absolute path
    ///   and use `Resource::Path`; the bytes are not duplicated in the binary.
    /// - Custom image → use the resolved file path through the same disk loader.
    /// - Video selection → falls back to the default builtin image asset.
    ///   Task 10 will replace this branch with the live video render path; in
    ///   the meantime we never hand GPUI a video byte stream through `img()`.
    /// - Anything unresolvable (deleted custom file, missing data root) →
    ///   falls back to the default builtin image asset so the canvas never
    ///   renders a blank backdrop.
    pub(crate) fn image_source_for_render(&self) -> gpui::ImageSource {
        Self::image_source_for_selection(&self.selection)
    }

    pub(crate) fn image_source_for_selection(selection: &WallpaperSelection) -> gpui::ImageSource {
        match selection_kind(selection) {
            WallpaperKind::Video => default_wallpaper_image_source(),
            WallpaperKind::Static | WallpaperKind::Gif => resolve_selection_path(selection)
                .map(gpui::ImageSource::from)
                .unwrap_or_else(default_wallpaper_image_source),
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
                // Unknown extensions don't map to a WallpaperKind; bail to filter them out.
                WallpaperKind::from_extension(ext)?;
                path.file_name().and_then(|n| n.to_str()).map(str::to_owned)
            })
            .collect();
        out.sort();
        out
    }

    /// Delete a custom wallpaper file from disk. If the deleted file was the
    /// current selection, reset to the default builtin and persist the change.
    /// Caller must invoke `cx.notify()` to refresh the view.
    #[cfg(test)]
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

    /// Import an arbitrary file as a custom wallpaper.
    ///
    /// Steps:
    /// 1. Derive [`WallpaperKind`] from the source extension via
    ///    [`WallpaperKind::from_extension`]. Unknown extension →
    ///    [`ImportResult::Rejected`]; nothing is written to disk.
    /// 2. Ensure [`custom_wallpapers_dir`] exists (`create_dir_all`).
    /// 3. Pick a non-colliding basename: if a file with the same name already
    ///    exists in the custom dir, append `_2`, `_3`, … before the extension
    ///    until a free name is found.
    /// 4. `fs::copy` the source into the custom dir. IO error →
    ///    [`ImportResult::Rejected`] with the error message; the half-written
    ///    destination (if any) is best-effort removed.
    /// 5. [`select_custom`] the new filename (which persists the selection) and
    ///    return [`ImportResult::Imported`] carrying the final filename.
    ///
    /// The caller must invoke `cx.notify()` to refresh the view.
    #[cfg(test)]
    pub(crate) fn import_custom(&mut self, src: &Path) -> ImportResult {
        let result = Self::copy_custom(src);
        if let ImportResult::Imported(filename) = &result {
            self.select_custom(filename.clone());
        }
        result
    }

    pub(crate) fn copy_custom(src: &Path) -> ImportResult {
        let Some(ext) = src.extension().and_then(|e| e.to_str()) else {
            return ImportResult::Rejected("file has no extension".into());
        };
        if WallpaperKind::from_extension(ext).is_none() {
            return ImportResult::Rejected(format!("unsupported extension: .{ext}"));
        }

        let Some(dir) = custom_wallpapers_dir() else {
            return ImportResult::Rejected("custom wallpaper data dir unavailable".into());
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return ImportResult::Rejected(format!("create_dir_all failed: {e}"));
        }

        let dest = non_colliding_dest(&dir, src);
        if let Err(e) = std::fs::copy(src, &dest) {
            // Best-effort cleanup of a partial copy so a failed import never
            // surfaces as a broken thumbnail in list_custom.
            let _ = std::fs::remove_file(&dest);
            return ImportResult::Rejected(format!("copy failed: {e}"));
        }

        let filename = dest
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| "imported".to_string());
        ImportResult::Imported(filename)
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

fn default_wallpaper_image_source() -> gpui::ImageSource {
    resolve_builtin_asset(DEFAULT_WALLPAPER_ASSET)
        .map(gpui::ImageSource::from)
        .unwrap_or_else(|| gpui::ImageSource::from(DEFAULT_WALLPAPER_ASSET))
}

/// Read both keys and reconstruct a selection. Returns None when either
/// key is missing, the kind is unknown, the builtin rel is not in the
/// registry, or the custom filename is malformed (empty / contains a path
/// separator). The caller decides whether to fall back to default.
#[derive(Debug)]
struct LoadedSelection {
    selection: WallpaperSelection,
    needs_persist: bool,
}

fn load_saved_selection(store: &SettingsStore) -> storage::StorageResult<Option<LoadedSelection>> {
    let kind: Option<String> = store.get_string(KEY_WALLPAPER_KIND)?;
    let value: Option<String> = store.get_string(KEY_WALLPAPER_VALUE)?;
    let (Some(kind), Some(value)) = (kind, value) else {
        return Ok(None);
    };
    let value_str = value.as_str();
    let (selection, needs_persist) = match kind.as_str() {
        KIND_BUILTIN => {
            let rel = canonical_builtin_wallpaper(value_str);
            match rel {
                Some(r) => (WallpaperSelection::Builtin { rel: r }, r != value_str),
                None => return Ok(None),
            }
        }
        KIND_CUSTOM => {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains('\\') {
                return Ok(None);
            }
            (
                WallpaperSelection::Custom {
                    filename: trimmed.to_string(),
                },
                false,
            )
        }
        _ => return Ok(None),
    };
    Ok(Some(LoadedSelection {
        selection,
        needs_persist,
    }))
}

/// Classify an arbitrary selection's WallpaperKind. Unknown builtin rel
/// (shouldn't happen — builtins come from the registry) defaults to Static.
/// Unknown custom extension also defaults to Static rather than being
/// unrenderable, matching the strictest-but-safe behavior.
pub(super) fn selection_kind(sel: &WallpaperSelection) -> WallpaperKind {
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

pub(crate) fn resolve_selection_path(selection: &WallpaperSelection) -> Option<PathBuf> {
    match selection {
        WallpaperSelection::Builtin { rel } => resolve_builtin_asset(rel),
        WallpaperSelection::Custom { filename } => {
            let path = custom_wallpapers_dir()?.join(filename);
            path.is_file().then_some(path)
        }
    }
}

/// Whether the selection can be resolved to a real on-disk asset right now.
/// Registered builtin choices remain valid while optional media is offline;
/// custom entries need the file present in the custom dir. Used at load time to discard
/// stale persisted selections (e.g. user deleted a custom file out of band).
fn selection_resolves(sel: &WallpaperSelection) -> bool {
    match sel {
        WallpaperSelection::Builtin { rel } => canonical_builtin_wallpaper(rel).is_some(),
        WallpaperSelection::Custom { filename } => custom_wallpapers_dir()
            .map(|d| d.join(filename).is_file())
            .unwrap_or(false),
    }
}

impl Default for WallpaperSelection {
    fn default() -> Self {
        Self::Builtin {
            rel: DEFAULT_WALLPAPER_ASSET,
        }
    }
}

impl WallpaperSelection {
    pub(crate) fn render_id_key(&self) -> String {
        match self {
            Self::Builtin { rel } => format!("builtin:{rel}"),
            Self::Custom { filename } => format!("custom:{filename}"),
        }
    }

    pub(crate) fn reference(&self) -> storage::WallpaperReference {
        match self {
            Self::Builtin { rel } => storage::WallpaperReference::Builtin { rel: (*rel).into() },
            Self::Custom { filename } => storage::WallpaperReference::Custom {
                filename: filename.clone(),
            },
        }
    }
}
