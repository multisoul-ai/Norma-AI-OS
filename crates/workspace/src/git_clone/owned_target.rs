//! Clone target creation and identity-bound cleanup ownership.
#[cfg(all(test, windows))]
use std::process::Command;
use std::{fs, path::PathBuf};

#[cfg(test)]
#[path = "../../tests/unit/git_clone_target_ownership.rs"]
mod tests;

pub(super) struct OwnedTarget {
    path: PathBuf,
    #[cfg(windows)]
    identity: (u32, u64),
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

#[derive(Debug)]
pub(super) struct CreationFailure {
    pub(super) error: std::io::Error,
    pub(super) preserved: bool,
}

impl From<std::io::Error> for CreationFailure {
    fn from(error: std::io::Error) -> Self {
        Self {
            error,
            preserved: false,
        }
    }
}

impl OwnedTarget {
    pub(super) fn create(path: PathBuf) -> Result<Self, CreationFailure> {
        Self::create_with(path, || {})
    }

    #[cfg(windows)]
    fn create_with(path: PathBuf, on_created: impl FnOnce()) -> Result<Self, CreationFailure> {
        let invalid = || std::io::Error::from(std::io::ErrorKind::InvalidInput);
        let parent = path.parent().ok_or_else(invalid)?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(invalid)?;
        let parent = crate::scoped_directory::Directory::open(parent, true)?;
        // FILE_CREATE returns this new directory's pinned handle; no path reopen can adopt a replacement.
        let created = parent.create_child(name)?;
        on_created();
        let identity = created.identity().map_err(|error| CreationFailure {
            error,
            preserved: true,
        })?;
        Ok(Self { path, identity })
    }

    #[cfg(not(windows))]
    fn create_with(path: PathBuf, on_created: impl FnOnce()) -> Result<Self, CreationFailure> {
        fs::create_dir(&path)?;
        on_created();
        // Failed ownership verification must not unlink an unverified pathname.
        Self::capture(path).map_err(|error| CreationFailure {
            error,
            preserved: true,
        })
    }

    #[cfg(any(not(windows), test))]
    fn capture(path: PathBuf) -> std::io::Result<Self> {
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "clone target is not an owned directory",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(Self {
                path,
                device: metadata.dev(),
                inode: metadata.ino(),
            })
        }
        #[cfg(not(unix))]
        {
            #[cfg(windows)]
            let identity = crate::scoped_directory::Directory::open(&path, false)?.identity()?;
            Ok(Self {
                path,
                #[cfg(windows)]
                identity,
            })
        }
    }

    pub(super) fn cleanup(&self) {
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.dev() != self.device || metadata.ino() != self.inode {
                return;
            }
        }
        #[cfg(windows)]
        {
            let Some(parent) = self.path.parent() else {
                return;
            };
            let Some(name) = self.path.file_name().and_then(|name| name.to_str()) else {
                return;
            };
            if let Ok(parent) = crate::scoped_directory::Directory::open(parent, true) {
                let _ = parent.remove_child_tree(name, self.identity);
            }
        }
        #[cfg(not(windows))]
        let _ = fs::remove_dir_all(&self.path);
    }
}
