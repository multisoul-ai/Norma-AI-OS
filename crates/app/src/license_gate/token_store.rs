//! Offline token file under Application Support (`…/license/offline.token`).

use std::fs;
use std::path::{Path, PathBuf};
use storage::default_app_data_dir;

pub fn default_token_path() -> anyhow::Result<PathBuf> {
    Ok(default_app_data_dir()?
        .join("license")
        .join("offline.token"))
}

pub fn read_token(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn write_token(path: &Path, token: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{}\n", token.trim()))?;
    Ok(())
}
