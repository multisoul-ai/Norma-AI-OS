//! storage — SQLite-backed persistence for app and project state.

pub mod agent_definition_store;
pub mod agent_dispatch_store;
pub mod agent_task_store;
pub mod app_store;
pub mod background_agent_dispatch;
pub mod background_agent_journal;
pub mod canvas_store;
pub mod error;
pub mod layout_store;
pub mod migrations;
pub mod node_session_store;
pub mod project_store;
pub mod recent_store;
pub mod session_store;
pub mod settings_store;
pub mod voice_history_store;

pub use agent_definition_store::{
    AgentDefinition, AgentDefinitionOrigin, AgentDefinitionStore, AgentDefinitionUpdate,
    NewAgentDefinition,
};
pub use agent_dispatch_store::{
    AgentDispatchTaskState, AgentDispatchTaskStore, AgentDispatchTracking, NewAgentDispatchTask,
    StoredAgentDispatchTask,
};
pub use agent_task_store::{AgentTaskStore, StoredAgentTask};
pub use app_store::{default_app_data_dir, default_app_db_path, legacy_app_data_dir, AppStore};
pub use background_agent_dispatch::BackgroundAgentDispatchStore;
pub use background_agent_journal::BackgroundAgentTaskJournal;
pub use canvas_store::CanvasStore;
pub use error::{StorageError, StorageResult};
pub use layout_store::{LayoutMode, LayoutOriginalBounds, LayoutStateSnapshot, LayoutStore};
pub use node_session_store::{NodeSession, NodeSessionKind, NodeSessionStore};
pub use project_store::ProjectStore;
pub use recent_store::RecentStore;
pub use session_store::{SessionCheckpoint, SessionCheckpointKind, SessionStore};
pub use settings_store::SettingsStore;
pub use voice_history_store::{VoiceHistoryEntry, VoiceHistoryRecord, VoiceHistoryStore};

pub(crate) fn current_unix_timestamp() -> StorageResult<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}
