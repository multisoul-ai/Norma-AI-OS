use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::Command;

const NORMA_EXCLUDE_PATTERN: &str = ".multisoul/";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClonedWorkspaceSecurityError {
    TrackedNormaState,
    GitInspectionFailed(String),
    UnsafeGitMetadata,
    CouldNotUpdateLocalExclude(String),
}

impl fmt::Display for ClonedWorkspaceSecurityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TrackedNormaState => formatter.write_str(
                "This repository contains tracked Norma workspace state. The clone was kept, but it was not opened automatically.",
            ),
            Self::GitInspectionFailed(message) => {
                write!(formatter, "Could not inspect the cloned Git repository: {message}")
            }
            Self::UnsafeGitMetadata => formatter.write_str(
                "The cloned repository has an unsafe Git metadata layout and was not opened.",
            ),
            Self::CouldNotUpdateLocalExclude(message) => write!(
                formatter,
                "Could not protect local Norma workspace state from Git: {message}"
            ),
        }
    }
}

impl std::error::Error for ClonedWorkspaceSecurityError {}

pub fn secure_cloned_workspace(root: &Path) -> Result<(), ClonedWorkspaceSecurityError> {
    let tracked_output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--", ".multisoul"])
        .output()
        .map_err(|error| ClonedWorkspaceSecurityError::GitInspectionFailed(error.to_string()))?;
    if !tracked_output.status.success() {
        let message = String::from_utf8_lossy(&tracked_output.stderr)
            .trim()
            .to_string();
        return Err(ClonedWorkspaceSecurityError::GitInspectionFailed(
            if message.is_empty() {
                format!("Git exited with status {}", tracked_output.status)
            } else {
                message
            },
        ));
    }
    if !tracked_output.stdout.is_empty() {
        return Err(ClonedWorkspaceSecurityError::TrackedNormaState);
    }

    update_local_exclude(root)
}

fn update_local_exclude(root: &Path) -> Result<(), ClonedWorkspaceSecurityError> {
    let git_dir = root.join(".git");
    reject_symlink_or_non_directory(&git_dir)?;
    let info_dir = git_dir.join("info");
    reject_symlink_or_non_directory(&info_dir)?;
    let exclude_path = info_dir.join("exclude");
    if exclude_path.exists() {
        let metadata = fs::symlink_metadata(&exclude_path)
            .map_err(|error| exclude_update_error(error.to_string()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ClonedWorkspaceSecurityError::UnsafeGitMetadata);
        }
    }

    let existing = fs::read_to_string(&exclude_path).unwrap_or_default();
    if existing
        .lines()
        .any(|line| line.trim() == NORMA_EXCLUDE_PATTERN)
    {
        return Ok(());
    }

    let mut exclude = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude_path)
        .map_err(|error| exclude_update_error(error.to_string()))?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        exclude
            .write_all(b"\n")
            .map_err(|error| exclude_update_error(error.to_string()))?;
    }
    writeln!(exclude, "{NORMA_EXCLUDE_PATTERN}")
        .map_err(|error| exclude_update_error(error.to_string()))
}

fn reject_symlink_or_non_directory(path: &Path) -> Result<(), ClonedWorkspaceSecurityError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| exclude_update_error(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        Err(ClonedWorkspaceSecurityError::UnsafeGitMetadata)
    } else {
        Ok(())
    }
}

fn exclude_update_error(message: String) -> ClonedWorkspaceSecurityError {
    ClonedWorkspaceSecurityError::CouldNotUpdateLocalExclude(message)
}
