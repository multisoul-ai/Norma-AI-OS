//! storage — SQLite-backed persistence for app and project state.

pub mod agent_conversation_tail_store;
pub mod agent_definition_store;
pub mod agent_dispatch_store;
pub mod app_store;
pub mod background_agent_dispatch;
pub mod background_conversation_tail;
pub mod canvas_store;
pub mod database_paths;
pub mod error;
pub mod layout_store;
pub mod migrations;
pub mod node_session_store;
pub mod orchestration_authorization_store;
pub mod orchestration_task_route_store;
pub mod project_store;
pub mod recent_store;
pub mod session_store;
pub mod settings_store;
pub mod voice_history_store;
pub mod workspace_identity_store;

pub use agent_conversation_tail_store::{
    AgentConversationTailStore, StoredAgentConversationTail, StoredConversationAvailability,
};
pub use agent_definition_store::{
    AgentDefinition, AgentDefinitionOrigin, AgentDefinitionStore, AgentDefinitionUpdate,
    NewAgentDefinition,
};
pub use agent_dispatch_store::{
    AgentDispatchTaskState, AgentDispatchTaskStore, AgentDispatchTracking, NewAgentDispatchTask,
    NewOrchestrationAgentDispatchTask, StoredAgentDispatchTask,
};
pub use app_store::{default_app_data_dir, default_app_db_path, legacy_app_data_dir, AppStore};
pub use background_agent_dispatch::BackgroundAgentDispatchStore;
pub use background_conversation_tail::BackgroundConversationTailStore;
pub use canvas_store::CanvasStore;
pub use database_paths::{DatabaseMode, DatabasePaths};
pub use error::{StorageError, StorageResult};
pub use layout_store::{LayoutMode, LayoutOriginalBounds, LayoutStateSnapshot, LayoutStore};
pub use node_session_store::{NodeSession, NodeSessionKind, NodeSessionStore};
pub use orchestration_authorization_store::{
    OrchestrationAuthorizationCache, OrchestrationAuthorizationRecord,
    OrchestrationAuthorizationStore,
};
pub use orchestration_task_route_store::{
    NewOrchestrationTaskRoute, OrchestrationLineageRejection,
    OrchestrationTaskOperationReservation, OrchestrationTaskReservation, OrchestrationTaskRoute,
    OrchestrationTaskRouteStore,
};
pub use project_store::ProjectStore;
pub use recent_store::RecentStore;
pub use session_store::{SessionCheckpoint, SessionCheckpointKind, SessionStore};
pub use settings_store::SettingsStore;
pub use voice_history_store::{VoiceHistoryEntry, VoiceHistoryRecord, VoiceHistoryStore};
pub use workspace_identity_store::{WorkspaceIdentity, WorkspaceIdentityStore};

pub(crate) fn current_unix_timestamp() -> StorageResult<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}
