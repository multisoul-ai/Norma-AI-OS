use crate::private_fs;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<(), std::io::Error> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "managed path has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_path = parent.join(format!(
        ".norma-install-{}-{sequence}.tmp",
        std::process::id()
    ));
    let result = (|| {
        let mut file = private_fs::create_file(&temp_path, mode)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        #[cfg(unix)]
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        drop(file);
        fs::rename(&temp_path, path)?;
        set_mode(path, mode)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

pub(super) fn set_mode(path: &Path, mode: u32) -> Result<(), std::io::Error> {
    private_fs::set_mode(path, mode)
}

#[cfg(test)]
#[path = "../../tests/unit/managed_files.rs"]
mod tests;

pub(super) fn remove_matching_binary(
    installed: &Path,
    bundled: &Path,
) -> Result<(), std::io::Error> {
    let Ok(installed_bytes) = fs::read(installed) else {
        return Ok(());
    };
    if fs::read(bundled).is_ok_and(|bundled_bytes| bundled_bytes == installed_bytes) {
        fs::remove_file(installed)?;
    }
    Ok(())
}
