//! Wallpaper import helpers (Task 8).
//!
//! Holds the [`ImportResult`] enum and the non-colliding destination resolver
//! used by [`super::state::WallpaperSettingsState::import_custom`]. Split out
//! of `state.rs` to respect the 500-line file cap; the import *method* itself
//! stays on the main `impl WallpaperSettingsState` block in `state.rs` so it
//! can call the private `persist` helper.

use std::path::{Path, PathBuf};

/// Outcome of importing a file as a custom wallpaper.
///
/// `Imported` carries the final basename the file was copied under (which may
/// differ from the source basename when a name collision was resolved).
/// `Rejected` carries a human-readable reason; no file is written to disk.
#[derive(Debug)]
pub(crate) enum ImportResult {
    Imported(String),
    Rejected(String),
}

/// Pick a non-colliding destination path inside `dir` for the given source
/// file. If `dir/<src basename>` is free, that is used directly; otherwise a
/// `_2`, `_3`, … suffix is inserted before the extension until a free name is
/// found. The returned path is not yet created — the caller performs the copy.
pub(super) fn non_colliding_dest(dir: &Path, src: &Path) -> PathBuf {
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| "imported".to_string());
    let ext = src
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_owned());

    let make = |suffix: &str| {
        let name = match &ext {
            Some(ext) => format!("{stem}{suffix}.{ext}"),
            None => format!("{stem}{suffix}"),
        };
        dir.join(name)
    };

    // First attempt: no suffix.
    let primary = make("");
    if !primary.exists() {
        return primary;
    }
    // Suffix loop: _2, _3, … capped to avoid an unbounded run.
    for n in 2..u32::MAX {
        let candidate = make(&format!("_{n}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    // Practically unreachable; fall back to primary to keep the function total.
    primary
}
