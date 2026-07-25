//! Rig-based, GPUI-free Norma Master Agent runtime.

mod adapter;
mod coordination;
mod current_context;
mod environment;
mod identity;
mod master;
mod master_prompt;
mod model;
mod protocol;
mod provider_catalog;
mod reply_visibility;
mod rig_tools;

pub use adapter::{
    norma_tool_definitions, AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter,
    NormaToolRuntime, ToolContext, ToolDefinition,
};
pub use coordination::{
    render_agent_roster, AgentAssignment, AgentCoordinatedTask, AgentCoordinationError,
    AgentDispatchReceipt, AgentDispatchSource, AgentDispatchState, AgentRosterEntry,
    AgentTaskCancellation, AgentTaskCoordinator, AgentTaskReferenceError, AgentTrackingMode,
};
pub use current_context::{
    render_current_canvas_context, CanvasCardContext, CanvasCardKind, CurrentCanvasContext,
    MAX_CURRENT_CANVAS_CARDS,
};
pub use environment::render_norma_environment;
pub use identity::{
    builtin_agent_names, AgentName, AgentNameError, AgentNameRegistry, BuiltinAgentName,
};
pub use master::{
    visible_norma_reply, ModelFactory, NormaCancelHandle, NormaSession, NormaSessionError,
    NormaTurnResult,
};
pub use master_prompt::norma_system_prompt;
pub use model::{AgentCommand, AgentEvent, TaskId, TaskState};
pub use protocol::{ProviderConfig, ProviderError, ProviderProtocol, RedactedProviderConfig};
pub use provider_catalog::{
    discover_provider_models, normalize_provider_models, validate_provider_model,
    ProviderCatalogError,
};
