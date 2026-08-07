//! Wallpaper data directory resolution and legacy migration helpers.

use std::path::{Path, PathBuf};

/// Env var overriding the data dir root for tests. When set, both the app db
/// (`<root>/app.sqlite`) and the custom wallpapers dir (`<root>/wallpapers`)
/// are routed underneath it. Production callers do not set this var and fall
/// back to `storage::default_app_data_dir()`.
pub(super) const TEST_DATA_ROOT_ENV: &str = "MULTISOUL_TEST_DATA_ROOT";

/// Absolute path to the custom wallpapers directory.
///
/// Production: `~/Library/Application Support/Norma OS/wallpapers/`.
/// Tests: `<MULTISOUL_TEST_DATA_ROOT>/wallpapers/`. Returns None when the
/// data root cannot be computed (rare: missing `$HOME` both in prod and when
/// env var unset).
pub(crate) fn custom_wallpapers_dir() -> Option<PathBuf> {
    let current_root = data_root()?;
    let legacy_root = legacy_data_root();
    custom_wallpapers_dir_from_roots(&current_root, legacy_root.as_deref())
}

pub(crate) fn custom_wallpapers_dir_from_roots(
    current_root: &Path,
    legacy_root: Option<&Path>,
) -> Option<PathBuf> {
    let current_dir = current_root.join("wallpapers");
    if current_dir.exists() {
        return Some(current_dir);
    }
    let Some(legacy_dir) = legacy_root.map(|root| root.join("wallpapers")) else {
        return Some(current_dir);
    };
    if !legacy_dir.exists() {
        return Some(current_dir);
    }
    if let Err(error) = copy_dir_all(&legacy_dir, &current_dir) {
        tracing::warn!(
            %error,
            source = %legacy_dir.display(),
            target = %current_dir.display(),
            "failed to migrate custom wallpapers directory"
        );
    }
    Some(current_dir)
}

/// Resolve the data dir root. Honors `MULTISOUL_TEST_DATA_ROOT` when set;
/// otherwise uses the current Norma OS app data dir.
pub(super) fn data_root() -> Option<PathBuf> {
    if let Some(test_root) = std::env::var(TEST_DATA_ROOT_ENV)
        .ok()
        .filter(|s| !s.is_empty())
    {
        return Some(PathBuf::from(test_root));
    }
    storage::default_app_data_dir().ok()
}

fn legacy_data_root() -> Option<PathBuf> {
    if std::env::var(TEST_DATA_ROOT_ENV).is_ok() {
        return None;
    }
    storage::legacy_app_data_dir().ok()
}

fn copy_dir_all(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target_path = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target_path)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target_path)?;
        }
    }
    Ok(())
}
