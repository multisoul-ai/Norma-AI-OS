//! Unified, GPUI-free Agent runtime for dialogue and serial card subagents.

mod coordination;
mod current_context;
mod diagnostics;
mod host;
mod identity;
mod journal;
mod lifecycle;
mod model;
mod prompts;
mod protocol;
mod provider;
mod providers;
mod system;
mod system_types;
mod tools;

pub use coordination::{
    render_agent_roster, AgentAssignment, AgentCoordinationError, AgentDispatchReceipt,
    AgentDispatchState, AgentRosterEntry, AgentTaskCoordinator, AgentTrackingMode,
};
pub use current_context::{
    render_current_canvas_context, CanvasCardContext, CanvasCardKind, CurrentCanvasContext,
};
pub use host::{CardToolError, CardToolHost};
pub use identity::{
    builtin_agent_names, AgentName, AgentNameError, AgentNameRegistry, BuiltinAgentName,
};
pub use journal::{AuditEntry, CardTaskJournal, MemoryJournal};
pub use model::{
    AgentCommand, AgentEvent, AgentMessage, CardToolRequest, CardToolResult, MessageRole,
    ProviderRequest, ProviderResponse, ProviderToolCall, ProviderToolSpec, TaskId, TaskRecord,
    TaskState, ToolCallRecord, ToolErrorRecord,
};
pub use prompts::{
    card_task_system_prompt, card_task_system_prompt_with_roster, dialogue_persona,
    dialogue_system_prompt, dialogue_system_prompt_with_context,
    dialogue_system_prompt_with_roster, MAX_DIALOGUE_HISTORY_TURNS,
};
pub use protocol::{
    build_ollama_chat_request, build_openai_chat_request, parse_ollama_response,
    parse_openai_response, ProviderConfig, ProviderProtocol, RedactedProviderConfig,
};
pub use provider::{AgentProvider, ProviderError, ProviderStreamEvent};
pub use providers::{
    parse_ollama_stream, parse_openai_stream, ConfiguredProvider, OllamaProvider,
    OpenAiCompatibleProvider,
};
pub use system::AgentSystem;
pub use system_types::{AgentError, CardTaskBudget};
pub use tools::{
    card_subagent_tool_specs, dialogue_tool_specs, parse_card_subagent_tool_call,
    parse_dialogue_tool_call, DialogueToolAction, CARD_TASK_TOOL,
};
