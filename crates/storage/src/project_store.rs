use crate::{
    migrations::require_workspace_dir, AgentConversationTailStore, AgentDefinitionStore,
    AgentDispatchTaskStore, CanvasStore, DatabasePaths, LayoutStore, NodeSessionStore,
    SessionStore, StorageError, StorageResult, VoiceHistoryStore,
};
use sha2::{Digest, Sha256};
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

    pub fn for_database_paths(paths: &DatabasePaths, root: &Path) -> StorageResult<Self> {
        require_workspace_dir(root)?;
        Ok(Self::new(paths.project_db_path(root)))
    }

    pub fn initialize_for_database_paths(
        paths: &DatabasePaths,
        root: &Path,
    ) -> StorageResult<Self> {
        let path = paths.project_db_path(root);
        let store = Self::for_database_paths(paths, root)
            .map_err(|source| project_initialization_error(paths, path.clone(), source))?;
        store
            .migrate()
            .map_err(|source| project_initialization_error(paths, path, source))?;
        Ok(store)
    }

    pub fn initialize_for_desktop_area(
        paths: &DatabasePaths,
        root: &Path,
        desktop_workspace_id: &str,
        area_id: &str,
    ) -> StorageResult<Self> {
        require_workspace_dir(root)?;
        let path = desktop_canvas_path(paths.app_db_path(), desktop_workspace_id, area_id)?;
        let store = Self::new(path.clone());
        store
            .migrate()
            .map_err(|source| project_initialization_error(paths, path, source))?;
        Ok(store)
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

    pub fn layout_store(&self) -> LayoutStore {
        LayoutStore::new(self.path.clone())
    }

    pub fn session_store(&self) -> SessionStore {
        SessionStore::new(self.path.clone())
    }

    pub fn node_session_store(&self) -> NodeSessionStore {
        NodeSessionStore::new(self.path.clone())
    }

    pub fn voice_history_store(&self) -> VoiceHistoryStore {
        VoiceHistoryStore::new(self.path.clone())
    }

    pub fn agent_definition_store(&self) -> AgentDefinitionStore {
        AgentDefinitionStore::new(self.path.clone())
    }

    pub fn agent_dispatch_task_store(&self) -> AgentDispatchTaskStore {
        AgentDispatchTaskStore::new(self.path.clone())
    }

    pub fn agent_conversation_tail_store(&self) -> AgentConversationTailStore {
        AgentConversationTailStore::new(self.path.clone())
    }
}

pub(crate) fn desktop_canvas_path(
    app_db_path: &Path,
    desktop_workspace_id: &str,
    area_id: &str,
) -> StorageResult<PathBuf> {
    let parent = app_db_path
        .parent()
        .ok_or_else(|| StorageError::InvalidAppDatabasePath(app_db_path.to_path_buf()))?;
    // Hash IDs before constructing a path so a corrupted catalog cannot traverse directories.
    let scope = format!("{desktop_workspace_id}:{area_id}");
    let filename = format!("{:x}.sqlite", Sha256::digest(scope.as_bytes()));
    Ok(parent.join("desktop-canvases").join(filename))
}

fn project_initialization_error(
    paths: &DatabasePaths,
    path: PathBuf,
    source: StorageError,
) -> StorageError {
    StorageError::DatabaseInitialization {
        mode: paths.mode(),
        path,
        source: Box::new(source),
    }
}
