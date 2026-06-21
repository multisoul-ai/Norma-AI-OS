use crate::{migrations::require_workspace_dir, CanvasStore, SessionStore, StorageResult};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ProjectStore {
    path: PathBuf,
}

impl ProjectStore {
    pub fn new(path: PathBuf) -> Self {
        ProjectStore { path }
    }

    pub fn for_workspace(root: &Path) -> StorageResult<Self> {
        require_workspace_dir(root)?;
        Ok(Self::new(root.join(".multisoul").join("project.sqlite")))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn migrate(&self) -> StorageResult<()> {
        let _connection = crate::migrations::open_project_connection(&self.path)?;
        Ok(())
    }

    pub fn canvas_store(&self) -> CanvasStore {
        CanvasStore::new(self.path.clone())
    }

    pub fn session_store(&self) -> SessionStore {
        SessionStore::new(self.path.clone())
    }
}
