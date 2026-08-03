use super::{NormaEndpointHost, RoutedTargets, WorkspaceTarget};
use crate::norma::route::WorkspaceRouteError;
use crate::norma::server::is_managed_agent_command;
use gpui::App;
use norma::protocol::{
    AgentErrorCode, AgentTaskReceiptStatus, CardCommand, CardCommandError, WireRequest,
    WireResponse, CURRENT_PROTOCOL_VERSION,
};

pub(super) fn validate_source(
    source_target: &WorkspaceTarget,
    request: &WireRequest,
    cx: &mut App,
) -> Result<(agent::AgentDispatchSource, norma::protocol::AgentProfile), WireResponse> {
    let request_id = request.request_id;
    let authorization = request.authorization.clone();
    let source_node_id = request.source_node_id;
    source_target
        .window
        .update(cx, |_root, _window, cx| {
            source_target.app.update(cx, |app, _cx| {
                app.validate_orchestration_source(authorization, source_node_id)
            })
        })
        .ok()
        .and_then(Result::ok)
        .ok_or_else(|| closed_workspace_response(request_id))?
        .map_err(|error| WireResponse::new(request_id, Err(error)))
}

pub(super) fn source_active_task_ref(
    source_target: &WorkspaceTarget,
    source: agent::AgentDispatchSource,
    cx: &mut App,
) -> Option<String> {
    source_target
        .window
        .update(cx, |_root, _window, cx| {
            source_target
                .app
                .update(cx, |app, _cx| app.active_orchestration_task_ref(source))
        })
        .ok()
        .and_then(Result::ok)
        .flatten()
}

pub(super) fn resolve_target_definition(
    target: &WorkspaceTarget,
    request: &WireRequest,
    cx: &mut App,
) -> Result<i64, WireResponse> {
    let request_id = request.request_id;
    target
        .window
        .update(cx, |_root, _window, cx| {
            target.app.update(cx, |app, _cx| {
                app.resolve_orchestration_target_definition(&request.command)
            })
        })
        .ok()
        .and_then(Result::ok)
        .ok_or_else(|| closed_workspace_response(request_id))?
        .map_err(|error| WireResponse::new(request_id, Err(error)))
}

pub(super) fn execute_validated_in_workspace(
    target: WorkspaceTarget,
    request_id: u64,
    command: CardCommand,
    source: agent::AgentDispatchSource,
    source_provider: norma::protocol::AgentProfile,
    visual_origin: Option<crate::motion::DispatchVisualOrigin>,
    cx: &mut App,
) -> WireResponse {
    target
        .window
        .update(cx, |_root, window, cx| {
            target.app.update(cx, |app, cx| {
                app.execute_validated_orchestration_command(
                    request_id,
                    command,
                    source,
                    source_provider,
                    visual_origin,
                    window,
                    cx,
                )
            })
        })
        .ok()
        .and_then(Result::ok)
        .unwrap_or_else(|| closed_workspace_response(request_id))
}

pub(super) fn resolve_targets(
    request: &WireRequest,
    cx: &App,
) -> Result<RoutedTargets, WorkspaceRouteError> {
    let Some(host) = cx.try_global::<NormaEndpointHost>() else {
        return Err(WorkspaceRouteError::WorkspaceNotOpen);
    };
    let source = match (
        request.source_workspace_id.as_deref(),
        request.source_workspace_route_id.as_deref(),
    ) {
        (Some(workspace_id), Some(route_id)) => {
            host.routes.resolve(workspace_id, route_id).cloned()
        }
        _ if is_managed_agent_command(&request.command) => {
            Err(WorkspaceRouteError::WorkspaceRequired)
        }
        _ => host
            .routes
            .sole_active_target()
            .cloned()
            .ok_or(WorkspaceRouteError::WorkspaceRequired),
    }?;
    let target = match request.target_workspace_id.as_deref() {
        Some(workspace_id) => host.routes.resolve_active(workspace_id).cloned()?,
        None => source.clone(),
    };
    Ok(RoutedTargets { source, target })
}

pub(super) fn execute_in_workspace(
    target: WorkspaceTarget,
    request: WireRequest,
    cx: &mut App,
) -> WireResponse {
    let request_id = request.request_id;
    target
        .window
        .update(cx, |_root, window, cx| {
            target
                .app
                .update(cx, |app, cx| app.execute_card_command(request, window, cx))
        })
        .ok()
        .and_then(Result::ok)
        .unwrap_or_else(|| closed_workspace_response(request_id))
}

pub(super) fn execute_cross_workspace(
    routed: RoutedTargets,
    request: WireRequest,
    cx: &mut App,
) -> WireResponse {
    let request_id = request.request_id;
    let validated = validate_source(&routed.source, &request, cx);
    let (_source, source_provider) = match validated {
        Ok(source) => source,
        Err(response) => return response,
    };
    execute_validated_in_workspace(
        routed.target,
        request_id,
        request.command,
        agent::AgentDispatchSource::norma(),
        source_provider,
        None,
        cx,
    )
}

pub(super) fn closed_workspace_response(request_id: u64) -> WireResponse {
    agent_error(
        request_id,
        AgentErrorCode::WorkspaceNotOpen,
        "目标 Workspace 窗口已关闭",
        true,
    )
}

pub(crate) fn protocol_compatibility_error(
    protocol_version: u32,
    managed_command: bool,
) -> Option<AgentErrorCode> {
    if !managed_command || protocol_version == CURRENT_PROTOCOL_VERSION {
        return None;
    }
    if protocol_version < CURRENT_PROTOCOL_VERSION {
        Some(AgentErrorCode::ClientUpgradeRequired)
    } else {
        Some(AgentErrorCode::ProtocolVersionMismatch)
    }
}

pub(super) fn routing_error_response(request_id: u64, error: WorkspaceRouteError) -> WireResponse {
    match error {
        WorkspaceRouteError::WorkspaceRequired => agent_error(
            request_id,
            AgentErrorCode::WorkspaceRequired,
            "请求必须携带 Workspace 身份；多窗口下不会猜测当前项目",
            false,
        ),
        WorkspaceRouteError::WorkspaceNotFound => agent_error(
            request_id,
            AgentErrorCode::WorkspaceNotFound,
            "找不到指定 Workspace",
            false,
        ),
        WorkspaceRouteError::WorkspaceNotOpen => agent_error(
            request_id,
            AgentErrorCode::WorkspaceNotOpen,
            "指定 Workspace 当前未打开",
            true,
        ),
        WorkspaceRouteError::RouteExpired => agent_error(
            request_id,
            AgentErrorCode::WorkspaceRouteExpired,
            "Workspace route 已过期，请从当前终端环境重新发起请求",
            true,
        ),
        WorkspaceRouteError::AlreadyOpen => agent_error(
            request_id,
            AgentErrorCode::InternalError,
            "Workspace route 注册冲突",
            false,
        ),
    }
}

pub(super) fn agent_error(
    request_id: u64,
    code: AgentErrorCode,
    message: &str,
    retryable: bool,
) -> WireResponse {
    WireResponse::new(
        request_id,
        Err(CardCommandError::agent(code, message, retryable)),
    )
}

pub(super) fn receipt_state(status: AgentTaskReceiptStatus) -> &'static str {
    match status {
        AgentTaskReceiptStatus::Done => "done",
        AgentTaskReceiptStatus::Failed => "failed",
        AgentTaskReceiptStatus::Cancelled => "cancelled",
        AgentTaskReceiptStatus::Interrupted => "interrupted",
        AgentTaskReceiptStatus::Queued => "queued",
        AgentTaskReceiptStatus::CancelRequested => "cancel_requested",
        AgentTaskReceiptStatus::Accepted => "accepted",
        AgentTaskReceiptStatus::Submitted | AgentTaskReceiptStatus::Untracked => "submitted",
    }
}

pub(super) fn serialized_profile(profile: norma::protocol::AgentProfile) -> String {
    serde_json::to_value(profile)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) fn response_error_code(response: &WireResponse) -> Option<&'static str> {
    let Err(CardCommandError::Agent(failure)) = &response.result else {
        return None;
    };
    Some(match failure.code {
        AgentErrorCode::UnknownAgent => "unknown_agent",
        AgentErrorCode::AgentDisabled => "agent_disabled",
        AgentErrorCode::EmptyTask => "empty_task",
        AgentErrorCode::UntrackedRequiresForce => "untracked_requires_force",
        _ => "internal_error",
    })
}
