use super::ContentError;
use super::{files::fingerprint, Result};
use std::{ffi::CString, io::Read, path::Path};

/// Exchange first, then validate the actual displaced inode. A concurrent external save
/// cannot disappear between a version check and rename; preserve both versions on conflict.
pub(super) fn replace_checked(
    file: tempfile::NamedTempFile,
    target: &Path,
    expected: &str,
) -> Result<()> {
    exchange(file.path(), target)?;
    let displaced = std::fs::File::open(file.path())
        .and_then(|f| {
            let mut bytes = Vec::new();
            f.take(super::MAX_TEXT_BYTES + 1).read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .map_err(|e| e.to_string());
    if displaced
        .as_ref()
        .is_ok_and(|bytes| fingerprint(bytes) == expected)
    {
        return Ok(());
    }
    let rollback = exchange(file.path(), target);
    let (_, recovery) = file.keep().map_err(|e| e.to_string())?;
    match rollback {
        Ok(()) => Err(ContentError::SaveConflictRestored { recovery }),
        Err(error) => Err(ContentError::SaveConflictRecovery {
            recovery,
            detail: error.to_string(),
        }),
    }
}

fn exchange(left: &Path, right: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let left = CString::new(left.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let right = CString::new(right.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    // SAFETY: paths are nul-terminated and live through this atomic filesystem call.
    let result = unsafe { libc::renamex_np(left.as_ptr(), right.as_ptr(), libc::RENAME_SWAP) };
    #[cfg(target_os = "linux")]
    // SAFETY: both names are nul-terminated; AT_FDCWD resolves absolute paths.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            left.as_ptr(),
            libc::AT_FDCWD,
            right.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let result = {
        let _ = (left, right);
        return Err(ContentError::AtomicUnsupported);
    };
    if result == 0 {
        Ok(())
    } else {
        Err(ContentError::Diagnostic(
            std::io::Error::last_os_error().to_string(),
        ))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_save_tests.rs"]
mod tests;
