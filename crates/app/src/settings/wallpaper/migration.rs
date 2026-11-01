//! Resolve one-time migration candidates at the asset boundary, where availability is known.
use super::state::{selection_resolves, WallpaperSelection};
use storage::{SettingsStore, StorageResult, WallpaperReference};

const KEY: &str = "wallpaper.global_candidates.v1";

pub(super) fn finish(store: &SettingsStore) -> StorageResult<()> {
    let Some(json) = store.get_string(KEY)? else {
        return Ok(());
    };
    let candidates: Vec<WallpaperReference> = serde_json::from_str(&json).unwrap_or_default();
    let current = match (
        store.get_string("wallpaper.kind")?,
        store.get_string("wallpaper.value")?,
    ) {
        (Some(kind), Some(value)) if kind == "builtin" => {
            Some(WallpaperReference::Builtin { rel: value })
        }
        (Some(kind), Some(value)) if kind == "custom" => {
            Some(WallpaperReference::Custom { filename: value })
        }
        _ => None,
    };
    let selection = current
        .into_iter()
        .chain(candidates)
        .filter_map(|reference| {
            reference.validate().ok()?;
            match reference {
                WallpaperReference::Builtin { rel } => {
                    crate::app::wallpaper_resolvers::canonical_builtin_wallpaper(&rel)
                        .map(|rel| WallpaperSelection::Builtin { rel })
                }
                WallpaperReference::Custom { filename } => {
                    Some(WallpaperSelection::Custom { filename })
                }
            }
        })
        .find(selection_resolves)
        .unwrap_or_default();
    let (kind, value) = match selection {
        WallpaperSelection::Builtin { rel } => ("builtin", rel.to_owned()),
        WallpaperSelection::Custom { filename } => ("custom", filename),
    };
    store
        .apply_string_changes_if_current(
            KEY,
            Some(&json),
            &[
                ("wallpaper.kind".into(), Some(kind.into())),
                ("wallpaper.value".into(), Some(value)),
                (KEY.into(), None),
            ],
        )
        .map(|_| ())
}
