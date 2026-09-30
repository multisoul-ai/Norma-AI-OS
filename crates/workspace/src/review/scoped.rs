use super::{ContentKind, FileVersion, ReviewError, MAX_FILE_BYTES};
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::Read,
    os::fd::{AsRawFd, FromRawFd},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path},
    process::Command,
};

pub(super) struct Root(File);
impl Root {
    pub(super) fn open(path: &Path) -> Result<Self, ReviewError> {
        let path = path.canonicalize().map_err(|_| ReviewError::Unavailable)?;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
            .map(Self)
            .map_err(|_| ReviewError::UnsafePath)
    }
    pub(super) fn cwd(&self, command: &mut Command) -> Result<(), ReviewError> {
        use std::os::unix::process::CommandExt;
        let directory = self.0.try_clone().map_err(|_| ReviewError::Unavailable)?;
        // SAFETY: pre_exec calls only async-signal-safe fchdir on a descriptor owned by this closure.
        unsafe {
            command.pre_exec(move || {
                if libc::fchdir(directory.as_raw_fd()) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(())
    }
    pub(super) fn read(&self, path: &str) -> Result<Option<FileVersion>, ReviewError> {
        validate(path)?;
        let mut directory = self.0.try_clone().map_err(|_| ReviewError::Unavailable)?;
        let parts = path.split('/').collect::<Vec<_>>();
        for name in &parts[..parts.len() - 1] {
            let Some(child) = openat(&directory, name, libc::O_DIRECTORY)? else {
                return Ok(None);
            };
            directory = child;
        }
        let name = CString::new(*parts.last().ok_or(ReviewError::UnsafePath)?)
            .map_err(|_| ReviewError::UnsafePath)?;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: name is NUL-terminated and stat points to enough writable storage; never follow links.
        if unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } < 0
        {
            return missing_or_unsafe();
        }
        // SAFETY: fstatat succeeded, so stat was fully initialized.
        let stat = unsafe { stat.assume_init() };
        if stat.st_mode & libc::S_IFMT == libc::S_IFLNK {
            let mut bytes = vec![0u8; 4097]; // PATH_MAX(4096)+1 detects an oversized symlink without following it.
                                             // SAFETY: readlinkat writes at most bytes.len() into this allocated buffer.
            let count = unsafe {
                libc::readlinkat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    bytes.as_mut_ptr().cast(),
                    bytes.len(),
                )
            };
            if count < 0 {
                return Err(ReviewError::Changed);
            }
            if count as usize >= bytes.len() {
                return Err(ReviewError::TooLarge);
            }
            bytes.truncate(count as usize);
            return Ok(Some(FileVersion {
                kind: ContentKind::Symlink,
                executable: false,
                bytes,
            }));
        }
        let Some(mut file) = openat(&directory, parts.last().expect("validated path"), 0)? else {
            return Ok(None);
        };
        let before = file.metadata().map_err(|_| ReviewError::Unavailable)?;
        if !before.is_file() || before.nlink() != 1 {
            return Err(ReviewError::UnsafePath);
        }
        if before.len() > MAX_FILE_BYTES as u64 {
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
        let after = file.metadata().map_err(|_| ReviewError::Unavailable)?;
        if (
            before.dev(),
            before.ino(),
            before.len(),
            before.mtime(),
            before.mtime_nsec(),
        ) != (
            after.dev(),
            after.ino(),
            after.len(),
            after.mtime(),
            after.mtime_nsec(),
        ) {
            return Err(ReviewError::Changed);
        }
        Ok(Some(FileVersion::regular(
            bytes,
            before.mode() & 0o111 != 0,
        )))
    }
}
pub(super) fn validate(path: &str) -> Result<(), ReviewError> {
    if path.is_empty()
        || path.len() > 4096
        || path.as_bytes().contains(&0)
        || is_internal(path)
        || path.split('/').any(str::is_empty)
        || Path::new(path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(ReviewError::UnsafePath);
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
fn openat(directory: &File, name: &str, flags: i32) -> Result<Option<File>, ReviewError> {
    let name = CString::new(name).map_err(|_| ReviewError::UnsafePath)?;
    // SAFETY: directory owns a live fd; flags prevent symlink following and blocking on special files.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK | flags,
        )
    };
    if fd < 0 {
        return missing_or_unsafe();
    }
    // SAFETY: successful openat returns a fresh owned descriptor.
    Ok(Some(unsafe { File::from_raw_fd(fd) }))
}
fn missing_or_unsafe<T>() -> Result<Option<T>, ReviewError> {
    if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
        Ok(None)
    } else {
        Err(ReviewError::UnsafePath)
    }
}
