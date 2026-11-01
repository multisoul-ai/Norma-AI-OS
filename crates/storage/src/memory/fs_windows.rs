use super::{MemoryError, MemoryResult};
use std::{fs::File, path::Path};

pub(super) struct Directory(workspace::scoped_directory::Directory);
fn convert(error: std::io::Error) -> MemoryError {
    match error.kind() {
        std::io::ErrorKind::InvalidInput => MemoryError::InvalidPath,
        std::io::ErrorKind::FileTooLarge => MemoryError::TooLarge,
        _ => MemoryError::Io(error),
    }
}
impl Directory {
    pub fn open(path: &Path) -> MemoryResult<Self> {
        workspace::scoped_directory::Directory::open(path, true)
            .map(Self)
            .map_err(convert)
    }
    pub fn child(&self, name: &str, create: bool) -> MemoryResult<Self> {
        let child = self.0.child(name, create).map_err(convert)?;
        if create {
            self.sync()?;
        }
        Ok(Self(child))
    }
    pub fn create_child(&self, name: &str) -> MemoryResult<Self> {
        self.0.create_child(name).map(Self).map_err(convert)
    }
    pub fn read(&self, name: &str, max: usize) -> MemoryResult<Vec<u8>> {
        self.0.read(name, max).map_err(convert)
    }
    pub fn write_new(&self, name: &str, bytes: &[u8]) -> MemoryResult<()> {
        self.0.write_new(name, bytes).map_err(convert)
    }
    pub fn rename(&self, from: &str, to: &str) -> MemoryResult<()> {
        self.0.rename(from, to).map_err(convert)
    }
    pub fn sync(&self) -> MemoryResult<()> {
        self.0.sync().map_err(convert)
    }
    pub fn lock(&self) -> MemoryResult<File> {
        let file = self.0.lock_file().map_err(convert)?;
        match file.try_lock() {
            Ok(()) => Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => Err(MemoryError::Busy),
            Err(std::fs::TryLockError::Error(error)) => Err(MemoryError::Io(error)),
        }
    }
}
