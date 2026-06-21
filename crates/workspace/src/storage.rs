use crate::{recent::canonical_workspace_path, RecentWorkspaces, WorkspaceSource};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceStorageError {
    #[error("could not determine user home directory for Application Support")]
    MissingHomeDirectory,
    #[error("workspace path is not an existing directory: {0}")]
    InvalidWorkspacePath(PathBuf),
    #[error("workspace storage io failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("workspace storage json failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("system time is before unix epoch: {0}")]
    Time(#[from] std::time::SystemTimeError),
}

pub struct RecentStorage {
    path: PathBuf,
}

impl RecentStorage {
    pub fn new(path: PathBuf) -> Self {
        RecentStorage { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<RecentWorkspaces, WorkspaceStorageError> {
        if !self.path.exists() {
            return Ok(RecentWorkspaces::default());
        }

        let bytes = std::fs::read(&self.path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save(&self, recent: &RecentWorkspaces) -> Result<(), WorkspaceStorageError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(recent)?;
        std::fs::write(&self.path, bytes)?;
        Ok(())
    }

    pub fn record_opened(
        &self,
        path: &Path,
        source: WorkspaceSource,
    ) -> Result<RecentWorkspaces, WorkspaceStorageError> {
        if !path.is_dir() {
            return Err(WorkspaceStorageError::InvalidWorkspacePath(
                path.to_path_buf(),
            ));
        }

        let canonical_path = canonical_workspace_path(path)?;
        let mut recent = self.load()?;
        recent.record_opened_at(canonical_path, source, current_unix_timestamp()?);
        self.save(&recent)?;
        Ok(recent)
    }
}

pub fn default_recent_workspaces_path() -> Result<PathBuf, WorkspaceStorageError> {
    let home = dirs::home_dir().ok_or(WorkspaceStorageError::MissingHomeDirectory)?;
    Ok(home
        .join("Library")
        .join("Application Support")
        .join("Multisoul")
        .join("recent-workspaces.json"))
}

fn current_unix_timestamp() -> Result<u64, WorkspaceStorageError> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
