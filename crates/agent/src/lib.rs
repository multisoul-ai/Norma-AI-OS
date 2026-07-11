//! Unified, GPUI-free Agent runtime for dialogue and serial card subagents.

mod diagnostics;
mod host;
mod journal;
mod lifecycle;
mod model;
mod protocol;
mod provider;
mod providers;
mod system;
mod system_types;
mod tools;

pub use host::{CardToolError, CardToolHost};
pub use journal::{AuditEntry, CardTaskJournal, MemoryJournal};
pub use model::{
    AgentCommand, AgentEvent, AgentMessage, CardToolRequest, CardToolResult, MessageRole,
    ProviderRequest, ProviderResponse, ProviderToolCall, ProviderToolSpec, TaskId, TaskRecord,
    TaskState, ToolCallRecord, ToolErrorRecord,
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
