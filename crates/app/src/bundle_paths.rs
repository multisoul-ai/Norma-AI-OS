//! Executable-relative resource layout shared by media and browser helpers.
use std::path::{Path, PathBuf};

pub(crate) fn resources_for_executable(executable: &Path) -> Option<PathBuf> {
    if !executable.is_absolute() {
        return None;
    }
    let binaries = executable.parent()?;
    #[cfg(windows)]
    {
        Some(binaries.join("Resources"))
    }
    #[cfg(not(windows))]
    {
        macos_resources(binaries)
    }
}

#[cfg(not(windows))]
fn macos_resources(binaries: &Path) -> Option<PathBuf> {
    let contents = binaries.parent()?;
    if binaries.file_name()? != "MacOS" || contents.file_name()? != "Contents" {
        return None;
    }
    if !contents.parent()?.file_name()?.to_str()?.ends_with(".app") {
        return None;
    }
    Some(contents.join("Resources"))
}

#[cfg(test)]
#[path = "../tests/unit/bundle_paths.rs"]
mod tests;
