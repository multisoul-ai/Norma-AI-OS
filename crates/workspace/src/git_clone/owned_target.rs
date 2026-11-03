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

impl OwnedTarget {
    pub(super) fn create(path: PathBuf) -> std::io::Result<Self> {
        Self::create_with(path, || {})
    }

    fn create_with(path: PathBuf, on_created: impl FnOnce()) -> std::io::Result<Self> {
        fs::create_dir(&path)?;
        on_created();
        Self::capture(path.clone()).inspect_err(|_| {
            let _ = fs::remove_dir(&path);
        })
    }

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
