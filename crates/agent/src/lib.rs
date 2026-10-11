//! Rig-based, GPUI-free Norma Master Agent runtime.

mod adapter;
pub mod card_instance;
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
mod memory;
mod memory_access;
mod model;
mod persona;
mod project_plan;
mod protocol;
mod provider_catalog;
mod realtime_intent;
mod reply_visibility;
mod rig_tools;
mod tool_policy;
pub mod workspace_plan;

pub use norma::{workspace_dispatch, workspace_operation};
pub use realtime_intent::realtime_command_authorized;
pub use tool_policy::{ToolCallBudget, ToolCallPolicy, MAX_TOOL_CALLS_PER_TURN};

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
    MAX_UNFINISHED_DESCENDANTS,
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
pub use master_prompt::{norma_system_prompt, norma_system_prompt_base};
pub use memory::{
    MemoryCommitReceipt, MemoryEntry, MemoryEntryId, MemoryEntryIdError, MemoryKind,
    MemoryMutation, MemoryRevision, MemoryScope, MemorySourceReference, MemorySummary,
};
pub use memory_access::{
    memory_text_chunk, MemoryCommand, MemoryPreferences, MemoryScopeAlias, MemorySessionContext,
    MemoryToolOutput, MEMORY_CATALOG_BYTES, MEMORY_TOOL_BYTES,
};
pub use model::{AgentCommand, AgentEvent, AgentInputSource, TaskId, TaskState};
pub use persona::{NormaPersona, PersonaTone, MAX_PERSONA_INSTRUCTION_CHARS};
pub use project_plan::{
    generate_project_plan, valid_plan_direction, PlanTodo, PlanTodoChoice, ProjectPlanDraft,
};
pub use protocol::{ProviderConfig, ProviderError, ProviderProtocol, RedactedProviderConfig};
pub use provider_catalog::{
    discover_provider_models, normalize_provider_models, validate_provider_model,
    ProviderCatalogError,
};
