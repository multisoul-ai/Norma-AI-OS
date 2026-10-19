//! storage — SQLite-backed persistence for app and project state.

pub mod agent_conversation_tail_store;
mod agent_creation_store;
pub mod agent_definition_store;
pub use agent_creation_store::{AgentCreationReceipt, AgentCreationStore};
pub mod agent_dispatch_store;
pub mod app_store;
pub mod approval_audit;
mod mobile_session_store;
mod mobile_task_baseline_store;
mod mobile_task_delivery_store;
mod mobile_turn_store;
pub use mobile_session_store::{MobileSession, MobileSessionKey, RecordedMobileSession};
pub use mobile_task_baseline_store::{MobileTaskBaselineStore, StoredTaskBaseline, TaskBaseline};
pub use mobile_task_delivery_store::{
    MobileTaskDeliveryStore, StoredTaskDelivery, TaskDeliveryContent, TaskDeliverySummary,
};
pub use mobile_turn_store::{
    MobileTurnKey, MobileTurnOutcome, MobileTurnState, MobileTurnStore, RecordedMobileTurn,
};
pub mod background_agent_dispatch;
pub mod background_conversation_tail;
pub mod canvas_store;
pub mod database_paths;
pub mod desktop_workspace_store;
pub mod error;
pub mod layout_store;
mod live_operation_store;
pub use live_operation_store::{
    LiveOperationKey, LiveOperationLease, LiveOperationReceipt, LiveOperationRequest,
    LiveOperationReservation, LiveOperationState, LiveOperationStore, LiveTaskFact, LiveTaskWatch,
};
pub mod memory;
mod memory_access;
pub use memory::{MemoryError, MemoryResult, MemorySnapshot, MemoryStore};
pub use memory_access::{MemoryAccess, MemorySourceDetail};
pub mod migrations;
pub mod mobile_access_store;
mod mobile_activity_store;
mod mobile_push_store;
pub use mobile_activity_store::{
    ActivityJournal, MobileActivityRegistration, MobileActivityState, MobileActivityStore,
};
pub use mobile_push_store::{
    MobilePushRegistration, MobilePushState, MobilePushStore, PushEnvironment,
};
pub mod mobile_command_store;
mod thread_message_store;
mod workspace_chat_store;
pub use mobile_access_store::{
    MobileAccessStore, MobileDeviceGrant, MobilePermission, MobileProjectAccess,
};
pub use mobile_command_store::{
    DeliveryAcceptance, DeliveryFeedback, MobileCommandReceipt, MobileCommandReservation,
    MobileCommandStore, NewMobileCommand, NewQueuedMessage, PlanTodoReservation, QueuedMessage,
    StoredTodoDecision, TodoDecisionRef,
};
pub use mobile_command_store::{
    StoredThreadCommand, ThreadCommandBinding, ThreadTaskExecution, ThreadTaskExecutionEvent,
};
pub use thread_message_store::{
    NewThreadMessage, StoredThreadMessage, ThreadMessageStore, ThreadReceipt,
};
pub use workspace_chat_store::{WorkspaceChatEvent, WorkspaceChatReceipt, WorkspaceChatStore};
pub mod node_session_store;
pub mod orchestration_authorization_store;
pub mod orchestration_task_route_store;
pub mod project_plan_store;
pub mod project_store;
pub use project_plan_store::{ProjectPlanPage, ProjectPlanStore, StoredProjectPlan};
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
    NewOrchestrationAgentDispatchTask, StoredAgentDispatchResult, StoredAgentDispatchTask,
};
pub use app_store::{default_app_data_dir, default_app_db_path, legacy_app_data_dir, AppStore};
pub use approval_audit::{ApprovalAuditEntry, ApprovalAuditStore, BackgroundApprovalAudit};
pub use background_agent_dispatch::BackgroundAgentDispatchStore;
pub use background_conversation_tail::BackgroundConversationTailStore;
pub use canvas_store::CanvasStore;
pub use database_paths::{DatabaseMode, DatabasePaths};
pub use desktop_workspace_store::{
    DesktopAreaPreview, DesktopProjectArea, DesktopWorkspace, DesktopWorkspaceStore,
    WallpaperReference, WallpaperUsage,
};
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
    OrchestrationTaskRouteCompletion, OrchestrationTaskRouteStore,
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

pub(crate) fn is_mobile_agent_profile(profile: &str) -> bool {
    matches!(profile, "Codex" | "Claude" | "Cursor")
}
