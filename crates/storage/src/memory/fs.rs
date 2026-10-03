//! Descriptor-relative POSIX IO: every managed component is opened without
//! following symlinks. Holding directory handles also closes path-swap races.

use super::{MemoryError, MemoryResult};
use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub(super) struct Directory(File);

impl Directory {
    pub fn open(path: &Path) -> MemoryResult<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        Ok(Self(file))
    }

    pub fn child(&self, name: &str, create: bool) -> MemoryResult<Self> {
        let name = component(name)?;
        if create {
            // SAFETY: CString is live and terminated; self owns a valid directory fd.
            let result = unsafe { libc::mkdirat(self.0.as_raw_fd(), name.as_ptr(), 0o700) };
            if result == 0 {
                self.sync()?;
            } else if std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        self.open_file(&name, libc::O_RDONLY | libc::O_DIRECTORY)
            .map(Self)
    }

    pub fn create_child(&self, name: &str) -> MemoryResult<Self> {
        let name = component(name)?;
        // SAFETY: valid directory fd, live terminated name, private directory mode.
        if unsafe { libc::mkdirat(self.0.as_raw_fd(), name.as_ptr(), 0o700) } < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        self.open_file(&name, libc::O_RDONLY | libc::O_DIRECTORY)
            .map(Self)
    }

    fn open_file(&self, name: &CString, flags: libc::c_int) -> MemoryResult<File> {
        // SAFETY: fd is owned, name is terminated, and mode is supplied for O_CREAT.
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: openat returned a new fd; ownership is transferred exactly once.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    pub fn read(&self, name: &str, max_bytes: usize) -> MemoryResult<Vec<u8>> {
        let file = self.open_file(&component(name)?, libc::O_RDONLY)?;
        require_regular(&file)?;
        let mut bytes = Vec::new();
        // Read one extra byte to detect oversize files without unbounded allocation.
        file.take(max_bytes as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(MemoryError::TooLarge);
        }
        Ok(bytes)
    }

    pub fn write_new(&self, name: &str, bytes: &[u8]) -> MemoryResult<()> {
        let mut file = self.open_file(
            &component(name)?,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        )?;
        require_regular(&file)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }

    pub fn lock(&self) -> MemoryResult<File> {
        let file = self.open_file(&component("LOCK")?, libc::O_RDWR | libc::O_CREAT)?;
        require_regular(&file)?;
        match file.try_lock() {
            Ok(()) => Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => Err(MemoryError::Busy),
            Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
        }
    }

    pub fn rename(&self, from: &str, to: &str) -> MemoryResult<()> {
        let from = component(from)?;
        let to = component(to)?;
        // SAFETY: both names are terminated single components and fd remains live.
        let result = unsafe {
            libc::renameat(
                self.0.as_raw_fd(),
                from.as_ptr(),
                self.0.as_raw_fd(),
                to.as_ptr(),
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }

    pub fn sync(&self) -> MemoryResult<()> {
        self.0.sync_all().map_err(Into::into)
    }
}

fn component(name: &str) -> MemoryResult<CString> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err(MemoryError::InvalidPath);
    }
    CString::new(name).map_err(|_| MemoryError::InvalidPath)
}

fn require_regular(file: &File) -> MemoryResult<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(MemoryError::InvalidPath);
    }
    Ok(())
}
