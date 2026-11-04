//! ReplaceFile preserves the actual displaced content before conflict validation.
use super::{ContentError, Result};
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

pub(super) fn exchange(left: &Path, right: &Path) -> Result<()> {
    let parent = right.parent().ok_or(ContentError::PathOutsideProject)?;
    let backup = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let backup = backup.into_temp_path().keep().map_err(|e| e.to_string())?;
    std::fs::remove_file(&backup).map_err(|e| e.to_string())?;
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let (target, replacement, recovery) = (wide(right), wide(left), wide(&backup));
    // All names live through the call. The OS saves the actual replaced file, including races.
    let replaced = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            replacement.as_ptr(),
            recovery.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        let error = std::io::Error::last_os_error();
        // ReplaceFile can fail after moving the original to backup (ERROR_UNABLE_TO_MOVE_REPLACEMENT_2).
        return Err(if backup.exists() {
            ContentError::SaveConflictRecovery {
                recovery: backup,
                detail: error.to_string(),
            }
        } else {
            error.to_string().into()
        });
    }
    let recovery = backup;
    std::fs::rename(&recovery, left).map_err(|error| ContentError::SaveConflictRecovery {
        recovery,
        detail: error.to_string(),
    })
}
