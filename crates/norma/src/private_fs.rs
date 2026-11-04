//! Owner-only managed files. Windows uses a protected DACL, never a chmod no-op.
use std::fs::File;
use std::io;
use std::path::Path;

#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(crate) fn create_file(path: &Path, mode: u32) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)
}

#[cfg(windows)]
pub(crate) fn create_file(path: &Path, _mode: u32) -> io::Result<File> {
    windows::create_file(path)
}

#[cfg(unix)]
pub(crate) fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(windows)]
pub(crate) fn set_mode(path: &Path, _mode: u32) -> io::Result<()> {
    windows::restrict_to_owner(path)
}

#[cfg(unix)]
pub(crate) fn permissions_are_private(path: &Path) -> io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o777 == 0o600)
}

#[cfg(windows)]
pub(crate) fn permissions_are_private(path: &Path) -> io::Result<bool> {
    windows::permissions_are_private(path)
}

#[cfg(unix)]
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    windows::create_private_dir(path)
}

/// Inspect the already-open token file, avoiding a second path lookup at the permission boundary.
#[cfg(windows)]
pub fn file_permissions_are_private(file: &File) -> io::Result<bool> {
    windows::file_permissions_are_private(file)
}
#[cfg(unix)]
pub fn file_permissions_are_private(file: &File) -> io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    Ok(file.metadata()?.permissions().mode() & 0o077 == 0)
}
