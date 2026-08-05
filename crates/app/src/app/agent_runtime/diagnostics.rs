use agent::{AgentCommand, AgentEvent, ProviderConfig};

const FLOW_MARKER: &str = "[DEBUG-agent-flow-7c1e]";

pub(super) fn runtime_spawn(runtime_id: u64, config: &ProviderConfig, persistent: bool) {
    tracing::debug!(
        runtime_id,
        protocol = ?config.protocol(),
        model = config.model(),
        api_key_configured = config.api_key().is_some_and(|key| !key.trim().is_empty()),
        persistent,
        "{FLOW_MARKER} Norma runtime spawning"
    );
}

pub(super) fn command_submit(runtime_id: u64, command: &AgentCommand) {
    tracing::debug!(
        runtime_id,
        command = command_name(command),
        "{FLOW_MARKER} app submitted command to Norma"
    );
}

pub(super) fn event_receive(runtime_id: u64, event: &AgentEvent) {
    tracing::debug!(
        runtime_id,
        event = event_name(event),
        task_id = event_task_id(event),
        "{FLOW_MARKER} app received Norma event"
    );
}

pub(super) fn turn_events(events: &[AgentEvent]) {
    tracing::debug!(
        event_count = events.len(),
        event_names = events.iter().map(event_name).collect::<Vec<_>>().join(","),
        "{FLOW_MARKER} voice turn received Norma response"
    );
}

pub(super) fn empty_turn_fallback() {
    tracing::warn!("{FLOW_MARKER} Norma turn had no visible final response");
}

fn command_name(command: &AgentCommand) -> &'static str {
    match command {
        AgentCommand::UpdateAgentRoster(_) => "update_agent_roster",
        AgentCommand::UpdateCurrentCanvasContext(_) => "update_current_canvas_context",
        AgentCommand::SwitchModel(_) => "switch_model",
        AgentCommand::UserTurn { .. } => "user_turn",
        AgentCommand::CancelLatest => "cancel_latest",
        AgentCommand::CancelTask { .. } => "cancel_task",
        AgentCommand::Status { .. } => "status",
        AgentCommand::CardActivity { .. } => "card_activity",
        AgentCommand::ProjectClosed => "project_closed",
    }
}

fn event_name(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::DialogueTextDelta { .. } => "dialogue_text_delta",
        AgentEvent::DialogueStreamFinished => "dialogue_stream_finished",
        AgentEvent::TaskQueued { .. } => "task_queued",
        AgentEvent::TaskRunning { .. } => "task_running",
        AgentEvent::TaskStatus { .. } => "task_status",
        AgentEvent::ToolStarted { .. } => "tool_started",
        AgentEvent::ToolFinished { .. } => "tool_finished",
        AgentEvent::TaskCancelRequested { .. } => "task_cancel_requested",
        AgentEvent::TaskSucceeded { .. } => "task_succeeded",
        AgentEvent::TaskPartiallyDispatched { .. } => "task_partially_dispatched",
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
        | AgentEvent::TaskPartiallyDispatched { task_id, .. }
        | AgentEvent::TaskFailed { task_id, .. }
        | AgentEvent::TaskCancelled { task_id, .. }
        | AgentEvent::TaskInterrupted { task_id } => Some(*task_id),
        AgentEvent::DialogueTextDelta { .. }
        | AgentEvent::DialogueStreamFinished
        | AgentEvent::CardActivityNotification { .. }
        | AgentEvent::ProviderError { .. } => None,
    }
}
