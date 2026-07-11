use super::AgentToolRequest;
use agent::{AgentCommand, AgentEvent, CardToolRequest, ProviderConfig};
use norma::protocol::CardCommand;

const FLOW_MARKER: &str = "[DEBUG-agent-flow-7c1e]";

pub(super) fn runtime_spawn(runtime_id: u64, config: &ProviderConfig, persistent: bool) {
    tracing::debug!(
        runtime_id,
        protocol = ?config.protocol(),
        model = config.model(),
        api_key_configured = config.api_key().is_some_and(|key| !key.trim().is_empty()),
        persistent,
        "{FLOW_MARKER} project agent runtime spawning"
    );
}

pub(super) fn command_submit(runtime_id: u64, command: &AgentCommand) {
    tracing::debug!(
        runtime_id,
        command = command_name(command),
        "{FLOW_MARKER} app submitted command to project agent"
    );
}

pub(super) fn worker_command(command: &AgentCommand, pending_task_count: usize) {
    tracing::debug!(
        command = command_name(command),
        pending_task_count,
        "{FLOW_MARKER} project agent worker received command"
    );
}

pub(super) fn worker_result(result: &Result<Vec<AgentEvent>, String>, pending_task_count: usize) {
    tracing::debug!(
        success = result.is_ok(),
        produced_event_count = result.as_ref().map_or(0, Vec::len),
        pending_task_count,
        "{FLOW_MARKER} project agent worker handled command"
    );
    if let Err(error) = result {
        tracing::warn!(%error, "{FLOW_MARKER} project agent command failed");
    }
}

pub(super) fn worker_pump(events: &[AgentEvent], pending_task_count: usize) {
    tracing::debug!(
        produced_event_count = events.len(),
        pending_task_count,
        event_names = event_names(events),
        "{FLOW_MARKER} project agent worker pumped card task"
    );
}

pub(super) fn tool_bridge_send(request: &CardToolRequest) {
    tracing::debug!(
        task_id = request.task_id,
        tool_call_id = request.tool_call_id,
        tool = card_command_name(&request.command),
        "{FLOW_MARKER} card subagent sent request to app tool bridge"
    );
}

pub(super) fn tool_bridge_receive(runtime_id: u64, request: &AgentToolRequest) {
    tracing::debug!(
        runtime_id,
        task_id = request.task_id,
        tool_call_id = request.tool_call_id,
        tool = card_command_name(&request.command),
        "{FLOW_MARKER} GPUI tool bridge received card command"
    );
}

pub(super) fn tool_bridge_complete(runtime_id: u64, request: &AgentToolRequest, success: bool) {
    tracing::debug!(
        runtime_id,
        task_id = request.task_id,
        tool_call_id = request.tool_call_id,
        tool = card_command_name(&request.command),
        success,
        "{FLOW_MARKER} GPUI tool bridge completed card command"
    );
}

pub(super) fn event_receive(runtime_id: u64, event: &AgentEvent) {
    tracing::debug!(
        runtime_id,
        event = event_name(event),
        task_id = event_task_id(event),
        "{FLOW_MARKER} app event pump received project agent event"
    );
}

pub(super) fn turn_events(events: &[AgentEvent]) {
    tracing::debug!(
        event_count = events.len(),
        event_names = event_names(events),
        "{FLOW_MARKER} voice turn received project agent response events"
    );
}

pub(super) fn empty_turn_fallback() {
    tracing::warn!(
        "{FLOW_MARKER} voice turn had no visible agent event and will use the dispatch fallback text"
    );
}

fn command_name(command: &AgentCommand) -> &'static str {
    match command {
        AgentCommand::UserTurn(_) => "user_turn",
        AgentCommand::DirectCardTask(_) => "direct_card_task",
        AgentCommand::CancelLatest => "cancel_latest",
        AgentCommand::CancelTask { .. } => "cancel_task",
        AgentCommand::Status { .. } => "status",
        AgentCommand::CardActivity { .. } => "card_activity",
        AgentCommand::ProjectClosed => "project_closed",
    }
}

fn card_command_name(command: &CardCommand) -> &'static str {
    match command {
        CardCommand::List => "list_cards",
        CardCommand::Create { .. } => "create_card",
        CardCommand::Focus { .. } => "focus_card",
        CardCommand::SendText { .. } => "send_text",
        CardCommand::Delete { .. } => "delete_card",
    }
}

fn event_names(events: &[AgentEvent]) -> String {
    events.iter().map(event_name).collect::<Vec<_>>().join(",")
}

fn event_name(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::DialogueTextDelta { .. } => "dialogue_text_delta",
        AgentEvent::TaskQueued { .. } => "task_queued",
        AgentEvent::TaskRunning { .. } => "task_running",
        AgentEvent::TaskStatus { .. } => "task_status",
        AgentEvent::ToolStarted { .. } => "tool_started",
        AgentEvent::ToolFinished { .. } => "tool_finished",
        AgentEvent::TaskCancelRequested { .. } => "task_cancel_requested",
        AgentEvent::TaskSucceeded { .. } => "task_succeeded",
        AgentEvent::TaskFailed { .. } => "task_failed",
        AgentEvent::TaskCancelled { .. } => "task_cancelled",
        AgentEvent::TaskInterrupted { .. } => "task_interrupted",
        AgentEvent::CardActivityNotification { .. } => "card_activity_notification",
        AgentEvent::ProviderError { .. } => "provider_error",
    }
}

fn event_task_id(event: &AgentEvent) -> Option<u64> {
    match event {
        AgentEvent::TaskQueued { task_id, .. }
        | AgentEvent::TaskRunning { task_id }
        | AgentEvent::TaskStatus { task_id, .. }
        | AgentEvent::ToolStarted { task_id, .. }
        | AgentEvent::ToolFinished { task_id, .. }
        | AgentEvent::TaskCancelRequested { task_id }
        | AgentEvent::TaskSucceeded { task_id, .. }
        | AgentEvent::TaskFailed { task_id, .. }
        | AgentEvent::TaskCancelled { task_id, .. }
        | AgentEvent::TaskInterrupted { task_id } => Some(*task_id),
        AgentEvent::DialogueTextDelta { .. }
        | AgentEvent::CardActivityNotification { .. }
        | AgentEvent::ProviderError { .. } => None,
    }
}
