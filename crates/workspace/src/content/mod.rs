//! Project-scoped file browsing and local Git operations. Call from a background executor.
use std::path::{Component, Path, PathBuf};

mod atomic;
mod error;
pub use error::ContentError;
mod files;
mod git;
mod mutation;
mod process;
pub use files::{FileEntry, FileOptions, TextFile, MAX_TEXT_BYTES};
pub use git::{Change, ChangeStatus, Comparison, GitSnapshot};
pub use mutation::GitAction;

pub type Result<T> = std::result::Result<T, ContentError>;

#[derive(Clone, Debug)]
pub struct Project {
    root: PathBuf,
}

impl Project {
    pub fn open(root: &Path) -> Result<Self> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if !root.is_dir() {
            return Err(ContentError::ProjectMissing);
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, relative: &Path) -> Result<PathBuf> {
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(ContentError::PathOutsideProject);
        }
        if relative.components().any(|c| c.as_os_str() == ".git") {
            return Err(ContentError::GitInternalPath);
        }
        let path = self
            .root
            .join(relative)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !path.starts_with(&self.root)
            || path
                .strip_prefix(&self.root)
                .is_ok_and(|p| p.components().any(|c| c.as_os_str() == ".git"))
        {
            return Err(ContentError::LinkOutsideProject);
        }
        Ok(path)
    }

    pub(super) fn repository(&self) -> Result<git2::Repository> {
        let repo = git2::Repository::discover(&self.root)
            .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?;
        let work = repo.workdir().ok_or(ContentError::BareRepository)?;
        if work.canonicalize().map_err(|e| e.to_string())? != self.root {
            return Err(ContentError::WorktreeRootRequired);
        }
        Ok(repo)
    }
}
