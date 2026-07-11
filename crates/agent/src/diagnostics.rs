use crate::{MessageRole, ProviderRequest, ProviderResponse, CARD_TASK_TOOL};

const FLOW_MARKER: &str = "[DEBUG-agent-flow-7c1e]";

pub(crate) fn dialogue_request(project_id: &str, request: &ProviderRequest) {
    let has_system_message = request
        .messages
        .iter()
        .any(|message| message.role == MessageRole::System);
    let tool_names = tool_spec_names(request);
    tracing::debug!(
        project_id,
        message_count = request.messages.len(),
        has_system_message,
        tool_names,
        "{FLOW_MARKER} dialogue provider request prepared"
    );
    if !has_system_message {
        tracing::warn!(
            project_id,
            "{FLOW_MARKER} dialogue provider request has no system message"
        );
    }
}

pub(crate) fn dialogue_response(project_id: &str, response: &ProviderResponse) {
    let tool_names = response_tool_names(response);
    let recognized_tool_calls = response
        .tool_calls
        .iter()
        .filter(|call| call.name == CARD_TASK_TOOL)
        .count();
    tracing::debug!(
        project_id,
        text_chars = response.text.chars().count(),
        tool_call_count = response.tool_calls.len(),
        recognized_tool_calls,
        tool_names,
        "{FLOW_MARKER} dialogue provider response normalized"
    );
    if !response.tool_calls.is_empty() && recognized_tool_calls == 0 {
        tracing::warn!(
            project_id,
            tool_names,
            "{FLOW_MARKER} dialogue response tool calls do not match card_task"
        );
    }
}

pub(crate) fn card_request(
    project_id: &str,
    task_id: u64,
    round: usize,
    request: &ProviderRequest,
) {
    let has_system_message = request
        .messages
        .iter()
        .any(|message| message.role == MessageRole::System);
    tracing::debug!(
        project_id,
        task_id,
        round,
        message_count = request.messages.len(),
        has_system_message,
        tool_names = tool_spec_names(request),
        "{FLOW_MARKER} card subagent provider request prepared"
    );
    if !has_system_message {
        tracing::warn!(
            project_id,
            task_id,
            round,
            "{FLOW_MARKER} card subagent provider request has no system message"
        );
    }
}

pub(crate) fn card_response(
    project_id: &str,
    task_id: u64,
    round: usize,
    completed_tool_calls: usize,
    response: &ProviderResponse,
) {
    tracing::debug!(
        project_id,
        task_id,
        round,
        completed_tool_calls,
        text_chars = response.text.chars().count(),
        tool_call_count = response.tool_calls.len(),
        tool_names = response_tool_names(response),
        "{FLOW_MARKER} card subagent provider response normalized"
    );
    if response.tool_calls.is_empty() && completed_tool_calls == 0 {
        tracing::warn!(
            project_id,
            task_id,
            round,
            "{FLOW_MARKER} card subagent returned text without executing any card tool"
        );
    }
}

fn tool_spec_names(request: &ProviderRequest) -> String {
    request
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn response_tool_names(response: &ProviderResponse) -> String {
    response
        .tool_calls
        .iter()
        .map(|call| call.name.as_str())
        .collect::<Vec<_>>()
        .join(",")
}
