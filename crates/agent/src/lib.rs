//! Rig-based, GPUI-free Norma Master Agent runtime.

mod adapter;
mod conversation_tail;
mod coordination;
mod current_context;
mod direct_address;
mod direct_address_turn;
mod environment;
mod identity;
mod master;
mod master_history;
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
pub use conversation_tail::{
    compare_tails_for_context, render_conversation_tails, truncate_tail_message,
    AgentConversationAvailability, AgentConversationTail, AgentConversationTaskState,
    AgentReplyReceipt, AgentReplyStatus, ConversationTailRenderHints, RenderedConversationTails,
    MAX_TAIL_BODY_CHARS_TOTAL, MAX_TAIL_MESSAGE_CHARS,
};
pub use coordination::{
    render_agent_roster, AgentAssignment, AgentCoordinatedTask, AgentCoordinationError,
    AgentDispatchReceipt, AgentDispatchSource, AgentDispatchState, AgentQueueSnapshot,
    AgentQueueTask, AgentQueueTaskSource, AgentRosterEntry, AgentTaskCancellation,
    AgentTaskCoordinator, AgentTaskReferenceError, AgentTaskState, AgentTrackingMode,
};
pub use current_context::{
    render_current_canvas_context, CanvasCardContext, CanvasCardKind, CurrentCanvasContext,
    MAX_CURRENT_CANVAS_CARDS,
};
pub use direct_address::{parse_direct_address, AddressedAgent, DirectAddressIntent};
pub use environment::{render_norma_environment, render_norma_environment_with_hints};
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
