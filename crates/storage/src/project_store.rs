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
    /// Background-only persisted member lookup. Does not open a Runtime or create missing canvases.
    pub fn desktop_activity_members(
        app_db: &Path,
        area: &crate::DesktopProjectArea,
    ) -> StorageResult<Vec<(canvas::CanvasNodeSnapshot, crate::AgentDefinition)>> {
        let canvas = desktop_canvas_path(app_db, &area.desktop_workspace_id, &area.id)?;
        let parent = app_db
            .parent()
            .ok_or_else(|| StorageError::InvalidAppDatabasePath(app_db.to_owned()))?;
        let definitions = parent.join("desktop-project-definitions").join(format!(
            "{:x}.sqlite",
            Sha256::digest(area.project_key.to_string_lossy().as_bytes())
        ));
        if !canvas.is_file() || !definitions.is_file() {
            return Ok(Vec::new());
        }
        let members = crate::AgentDefinitionStore::new(definitions).list()?;
        let document = crate::CanvasStore::new(canvas).load_snapshot()?;
        Ok(document
            .into_iter()
            .flat_map(|d| d.nodes)
            .filter_map(|node| {
                let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                let definition = members
                    .iter()
                    .find(|d| Some(d.id) == spec.definition_id)?
                    .clone();
                Some((node, definition))
            })
            .collect())
    }
    pub fn new(path: PathBuf) -> Self {
        ProjectStore { path }
    }

    pub fn for_workspace(root: &Path) -> StorageResult<Self> {
        Self::for_database_paths(&DatabasePaths::production()?, root)
    }

    /// Background-only: import legacy data before exposing writable child stores.
    pub fn for_database_paths(paths: &DatabasePaths, root: &Path) -> StorageResult<Self> {
        require_workspace_dir(root)?;
        let root = root.canonicalize()?;
        let store = Self::new(paths.project_db_path(&root));
        import_legacy_project(paths, &root, store.path()).map_err(|source| {
            project_initialization_error(paths, store.path().to_owned(), source)
        })?;
        Ok(store)
    }

    pub fn initialize_for_database_paths(
        paths: &DatabasePaths,
        root: &Path,
    ) -> StorageResult<Self> {
        let path = paths.project_db_path(root);
        let store = Self::for_database_paths(paths, root)
            .map_err(|source| project_initialization_error(paths, path.clone(), source))?;
        store.migrate().map_err(|source| {
            project_initialization_error(paths, store.path().to_owned(), source)
        })?;
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
            .map_err(|source| project_initialization_error(paths, path.clone(), source))?;
        let connection = crate::migrations::open_project_connection(&path)?;
        crate::migrations::desktop_canvas::migrate(&connection)
            .map_err(|source| project_initialization_error(paths, path, source))?;
        Ok(store)
    }

    pub fn initialize_for_desktop_definitions(
        paths: &DatabasePaths,
        project_key: &Path,
    ) -> StorageResult<Self> {
        let parent = paths
            .app_db_path()
            .parent()
            .ok_or_else(|| StorageError::InvalidAppDatabasePath(paths.app_db_path().to_owned()))?;
        let filename = format!(
            "{:x}.sqlite",
            Sha256::digest(project_key.to_string_lossy().as_bytes())
        );
        let path = parent.join("desktop-project-definitions").join(filename);
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

fn import_legacy_project(paths: &DatabasePaths, root: &Path, target: &Path) -> StorageResult<()> {
    if target.try_exists()? {
        return Ok(());
    }
    let legacy = paths.legacy_project_db_path(root);
    if !legacy.try_exists()? {
        return Ok(());
    }
    let parent = target
        .parent()
        .ok_or_else(|| StorageError::InvalidAppDatabasePath(target.to_owned()))?;
    std::fs::create_dir_all(parent)?;
    let staged = tempfile::NamedTempFile::new_in(parent)?;
    // SQLite backup includes committed WAL pages without modifying the legacy database.
    let source =
        rusqlite::Connection::open_with_flags(legacy, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.backup(rusqlite::DatabaseName::Main, staged.path(), None)?;
    drop(crate::migrations::open_project_connection(staged.path())?);
    staged.as_file().sync_all()?;
    match staged.persist_noclobber(target) {
        Ok(_) => std::fs::File::open(parent)?.sync_all()?,
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.error.into()),
    }
    Ok(())
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
