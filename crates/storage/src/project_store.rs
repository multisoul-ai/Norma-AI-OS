use crate::{
    migrations::require_workspace_dir, AgentConversationTailStore, AgentDefinitionStore,
    AgentDispatchTaskStore, CanvasStore, LayoutStore, NodeSessionStore, SessionStore,
    StorageResult, VoiceHistoryStore,
};
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
