use super::{ContentKind, FileVersion, ReviewError, MAX_FILE_BYTES};
use crate::scoped_directory::{self, Directory};
use std::{fs::File, io::Read, path::Path, process::Command};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
};

pub(super) struct Root(Directory);
impl Root {
    pub(super) fn open(path: &Path) -> Result<Self, ReviewError> {
        let path = path.canonicalize().map_err(|_| ReviewError::Unavailable)?;
        Directory::open(&path, false)
            .map(Self)
            .map_err(|_| ReviewError::UnsafePath)
    }
    pub(super) fn cwd(&self, command: &mut Command) -> Result<(), ReviewError> {
        // Directory retains non-delete-sharing handles for root and every ancestor until capture ends.
        // A Git path lookup therefore cannot be redirected by renaming any component of the root.
        command.current_dir(self.0.path().map_err(|_| ReviewError::Changed)?);
        Ok(())
    }
    pub(super) fn read(&self, path: &str) -> Result<Option<FileVersion>, ReviewError> {
        validate(path)?;
        let parts = path.split('/').collect::<Vec<_>>();
        let mut children = Vec::new();
        for name in &parts[..parts.len() - 1] {
            let parent = children.last().unwrap_or(&self.0);
            let child = match parent.child(name, false) {
                Ok(child) => child,
                Err(error) => return missing_or_unsafe(error),
            };
            check_name(&child.path().map_err(|_| ReviewError::UnsafePath)?)?;
            children.push(child);
        }
        let parent = children.last().unwrap_or(&self.0);
        let file = match parent.open_entry(parts.last().ok_or(ReviewError::UnsafePath)?) {
            Ok(file) => file,
            Err(error) => return missing_or_unsafe(error),
        };
        let path = scoped_directory::final_path(&file).map_err(|_| ReviewError::UnsafePath)?;
        check_name(&path)?;
        let information =
            scoped_directory::information(&file).map_err(|_| ReviewError::UnsafePath)?;
        if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            // The opened entry excludes FILE_SHARE_DELETE; its name and parents cannot be swapped.
            let target = std::fs::read_link(&path).map_err(|_| ReviewError::UnsafePath)?;
            let bytes = target
                .to_str()
                .ok_or(ReviewError::Unsupported)?
                .as_bytes()
                .to_vec();
            if bytes.len() > 4096 {
                return Err(ReviewError::TooLarge);
            }
            return Ok(Some(FileVersion {
                kind: ContentKind::Symlink,
                executable: false,
                bytes,
            }));
        }
        if information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            return Err(ReviewError::UnsafePath);
        }
        scoped_directory::require_regular(&file).map_err(|_| ReviewError::UnsafePath)?;
        read_regular(file).map(Some)
    }
}
fn check_name(path: &Path) -> Result<(), ReviewError> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(ReviewError::UnsafePath)?;
    // The handle's normalized long name catches 8.3 aliases of internal directories.
    if is_internal(name) {
        Err(ReviewError::UnsafePath)
    } else {
        Ok(())
    }
}
fn read_regular(mut file: File) -> Result<FileVersion, ReviewError> {
    let before = scoped_directory::information(&file).map_err(|_| ReviewError::Unavailable)?;
    let metadata = file.metadata().map_err(|_| ReviewError::Unavailable)?;
    if metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(ReviewError::TooLarge);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ReviewError::Unavailable)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ReviewError::TooLarge);
    }
    let after = scoped_directory::information(&file).map_err(|_| ReviewError::Unavailable)?;
    let identity = |i: &windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION| {
        (
            i.dwVolumeSerialNumber,
            i.nFileIndexHigh,
            i.nFileIndexLow,
            i.nFileSizeHigh,
            i.nFileSizeLow,
            i.ftLastWriteTime.dwHighDateTime,
            i.ftLastWriteTime.dwLowDateTime,
            i.nNumberOfLinks,
            i.dwFileAttributes,
        )
    };
    if identity(&before) != identity(&after) {
        return Err(ReviewError::Changed);
    }
    Ok(FileVersion::regular(bytes, false))
}
pub(super) fn validate(path: &str) -> Result<(), ReviewError> {
    if path.is_empty() || path.len() > 4096 || is_internal(path) {
        return Err(ReviewError::UnsafePath);
    }
    for name in path.split('/') {
        scoped_directory::validate_component(name).map_err(|_| ReviewError::UnsafePath)?;
    }
    Ok(())
}
pub(super) fn is_internal(path: &str) -> bool {
    path.split('/').any(|part| {
        [".git", ".norma", ".multisoul"]
            .iter()
            .any(|name| part.eq_ignore_ascii_case(name))
    })
}
fn missing_or_unsafe<T>(error: std::io::Error) -> Result<Option<T>, ReviewError> {
    if error.kind() == std::io::ErrorKind::NotFound {
        Ok(None)
    } else {
        Err(ReviewError::UnsafePath)
    }
}
