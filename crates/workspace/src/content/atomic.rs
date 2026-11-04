use super::ContentError;
use super::{files::fingerprint, Result};
#[cfg(unix)]
use std::ffi::CString;
use std::{io::Read, path::Path};
#[cfg(windows)]
#[path = "atomic_windows.rs"]
mod windows;
#[cfg(windows)]
use windows::exchange;

/// Exchange first, then validate the actual displaced inode. A concurrent external save
/// cannot disappear between a version check and rename; preserve both versions on conflict.
pub(super) fn replace_checked(
    file: tempfile::NamedTempFile,
    target: &Path,
    expected: &str,
) -> Result<()> {
    // ReplaceFileW opens the replacement without sharing: release its writable handle first.
    // TempPath keeps cleanup/recovery ownership while allowing the OS to open both names.
    let mut file = file.into_temp_path();
    if let Err(error) = exchange(&file, target) {
        if matches!(&error, ContentError::SaveConflictRecovery { .. }) {
            file.disable_cleanup(true);
        }
        return Err(error);
    }
    let displaced = std::fs::File::open(&file)
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
    // Either name can now hold unique user content. Even a failed keep must never unlink it.
    file.disable_cleanup(true);
    let rollback = exchange(&file, target);
    if matches!(&rollback, Err(ContentError::SaveConflictRecovery { .. })) {
        // A partial Windows replacement already retained the actual original at its recovery path.
        // Its error takes precedence over a temporary pathname that may no longer exist.
        let _ = file.keep();
        return rollback;
    }
    conflict_result(file, rollback)
}

fn conflict_result(mut file: tempfile::TempPath, rollback: Result<()>) -> Result<()> {
    file.disable_cleanup(true);
    let recovery = file.to_path_buf();
    let warning = file.keep().err().map(|error| error.to_string());
    match rollback {
        Ok(()) => Err(ContentError::SaveConflictRestored { recovery, warning }),
        Err(error) => Err(ContentError::SaveConflictRecovery {
            recovery,
            detail: match warning {
                Some(warning) => format!("{error}; {warning}"),
                None => error.to_string(),
            },
        }),
    }
}

#[cfg(unix)]
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
