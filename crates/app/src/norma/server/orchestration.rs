use crate::app::MultisoulApp;
use canvas::{AgentProfile as CanvasAgentProfile, CanvasNodeKind};
use gpui::{Context, Window};
use norma::protocol::{
    AgentErrorCode, AgentProfile, CardCommand, CardCommandError, CardCommandResult, EgoErrorCode,
    EgoPresentationDisposition, NodeId, PresentationDisposition, PresentationErrorCode,
    PresentationTarget, ProviderAuthorization,
};

pub(crate) struct ValidatedOrchestrationCommand {
    pub(crate) request_id: u64,
    pub(crate) command: CardCommand,
    pub(crate) source: agent::AgentDispatchSource,
    pub(crate) source_provider: AgentProfile,
    pub(crate) visual_origin: Option<crate::motion::DispatchVisualOrigin>,
}
use std::time::Duration;

pub(crate) fn is_managed_agent_command(command: &CardCommand) -> bool {
    matches!(
        command,
        CardCommand::ListWorkspaces
            | CardCommand::ListAgents
            | CardCommand::DispatchAgentTasks { .. }
            | CardCommand::AgentTaskStatus { .. }
            | CardCommand::AgentTaskResult { .. }
            | CardCommand::AgentTaskCancel { .. }
            | CardCommand::Present { .. }
            | CardCommand::Ego { .. }
    )
}

pub(super) fn is_presentation_command(command: &CardCommand) -> bool {
    matches!(command, CardCommand::Present { .. })
}

impl MultisoulApp {
    pub(crate) fn resolve_orchestration_target_definition(
        &self,
        command: &CardCommand,
    ) -> Result<i64, CardCommandError> {
        let CardCommand::DispatchAgentTasks { assignments, .. } = command else {
            return Err(CardCommandError::agent(
                AgentErrorCode::InternalError,
                "全局任务预检只接受 Agent task assign",
                false,
            ));
        };
        let [assignment] = assignments.as_slice() else {
            return Err(CardCommandError::agent(
                AgentErrorCode::InternalError,
                "一次全局派发必须且只能包含一个目标 Agent",
                false,
            ));
        };
        if assignment.task.trim().is_empty() {
            return Err(CardCommandError::agent(
                AgentErrorCode::EmptyTask,
                "任务正文不能为空",
                false,
            ));
        }
        let definition = self
            .agent_definitions
            .definition_by_address(&assignment.name)
            .ok_or_else(|| {
                CardCommandError::agent(
                    AgentErrorCode::UnknownAgent,
                    format!("未找到 Agent：{}", assignment.name),
                    false,
                )
            })?;
        if !definition.enabled {
            return Err(CardCommandError::agent(
                AgentErrorCode::AgentDisabled,
                format!("Agent {} 已禁用", definition.name),
                false,
            ));
        }
        Ok(definition.id)
    }

    pub(crate) fn active_orchestration_task_ref(
        &self,
        source: agent::AgentDispatchSource,
    ) -> Option<String> {
        self.agent_task_coordinator
            .as_ref()
            .and_then(|coordinator| coordinator.active_task_ref_for_source(source))
            .map(str::to_owned)
    }

    pub(crate) fn execute_validated_orchestration_command(
        &mut self,
        request: ValidatedOrchestrationCommand,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> norma::protocol::WireResponse {
        let ValidatedOrchestrationCommand {
            request_id,
            command,
            source,
            source_provider,
            visual_origin,
        } = request;
        let result = crate::app::execute_orchestration_agent_command_with_visual_origin(
            self,
            window,
            command,
            source,
            source_provider,
            visual_origin,
        );
        if result.is_ok() {
            cx.notify();
        }
        norma::protocol::WireResponse::new(request_id, result)
    }

    pub(crate) fn validate_orchestration_source(
        &self,
        authorization: Option<ProviderAuthorization>,
        source_node_id: Option<NodeId>,
    ) -> Result<(agent::AgentDispatchSource, AgentProfile), CardCommandError> {
        let authorization = authorization.ok_or_else(|| {
            CardCommandError::agent(
                AgentErrorCode::Unauthorized,
                "Agent 编排请求缺少 Provider 授权",
                false,
            )
        })?;
        let provider = orchestration_provider(authorization.provider);
        if provider == norma::orchestration::OrchestrationProvider::Grok {
            return Err(CardCommandError::agent(
                AgentErrorCode::ProviderNotInstalled,
                "Grok Build 在当前版本不支持编排",
                false,
            ));
        }
        if !self
            .orchestration_authorizations
            .is_authorized(provider.slug(), &authorization.token)
        {
            return Err(CardCommandError::agent(
                AgentErrorCode::Unauthorized,
                "Provider 编排授权无效或已撤销",
                false,
            ));
        }
        let source_node_id = source_node_id.ok_or_else(|| {
            CardCommandError::agent(
                AgentErrorCode::SourceAgentUnavailable,
                "缺少来源 Agent 节点",
                false,
            )
        })?;
        let (source_definition_id, source_profile) = self
            .runtime
            .document
            .node(source_node_id)
            .and_then(|node| {
                let CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                spec.definition_id.map(|definition_id| {
                    (definition_id, norma_agent_profile_from_canvas(spec.profile))
                })
            })
            .ok_or_else(|| {
                CardCommandError::agent(
                    AgentErrorCode::SourceAgentUnavailable,
                    "来源 Agent 节点已关闭或无效",
                    false,
                )
            })?;
        let source_enabled = self
            .agent_definitions
            .definitions()
            .iter()
            .any(|definition| definition.id == source_definition_id && definition.enabled);
        if !source_enabled || source_profile != authorization.provider {
            return Err(CardCommandError::agent(
                AgentErrorCode::SourceAgentUnavailable,
                "来源 Agent 与 Provider 身份不匹配或已禁用",
                false,
            ));
        }
        Ok((
            agent::AgentDispatchSource::agent(source_definition_id, source_node_id),
            authorization.provider,
        ))
    }
}

fn orchestration_provider(profile: AgentProfile) -> norma::orchestration::OrchestrationProvider {
    match profile {
        AgentProfile::Claude => norma::orchestration::OrchestrationProvider::Claude,
        AgentProfile::Codex => norma::orchestration::OrchestrationProvider::Codex,
        AgentProfile::Cursor => norma::orchestration::OrchestrationProvider::Cursor,
        AgentProfile::Grok => norma::orchestration::OrchestrationProvider::Grok,
        AgentProfile::OpenCode => norma::orchestration::OrchestrationProvider::OpenCode,
        AgentProfile::InfCode => norma::orchestration::OrchestrationProvider::InfCode,
        AgentProfile::Kimi => norma::orchestration::OrchestrationProvider::Kimi,
        AgentProfile::ZCode => norma::orchestration::OrchestrationProvider::ZCode,
        AgentProfile::Pi => norma::orchestration::OrchestrationProvider::Pi,
        AgentProfile::Hermes => norma::orchestration::OrchestrationProvider::Hermes,
    }
}

fn norma_agent_profile_from_canvas(profile: CanvasAgentProfile) -> AgentProfile {
    match profile {
        CanvasAgentProfile::Claude => AgentProfile::Claude,
        CanvasAgentProfile::Codex => AgentProfile::Codex,
        CanvasAgentProfile::Cursor => AgentProfile::Cursor,
        CanvasAgentProfile::Grok => AgentProfile::Grok,
        CanvasAgentProfile::OpenCode => AgentProfile::OpenCode,
        CanvasAgentProfile::InfCode => AgentProfile::InfCode,
        CanvasAgentProfile::Kimi => AgentProfile::Kimi,
        CanvasAgentProfile::ZCode => AgentProfile::ZCode,
        CanvasAgentProfile::Pi => AgentProfile::Pi,
        CanvasAgentProfile::Hermes => AgentProfile::Hermes,
    }
}

pub(super) fn managed_command_label(command: &CardCommand) -> &'static str {
    match command {
        CardCommand::ListWorkspaces => "workspace_list",
        CardCommand::Doctor => "doctor",
        CardCommand::ListAgents => "agent_list",
        CardCommand::DispatchAgentTasks { .. } => "agent_task_assign",
        CardCommand::AgentTaskStatus { .. } => "agent_task_status",
        CardCommand::AgentTaskResult { .. } => "agent_task_result",
        CardCommand::AgentTaskCancel { .. } => "agent_task_cancel",
        CardCommand::Present {
            target: PresentationTarget::Ego,
            ..
        } => "present_ego",
        CardCommand::Present {
            target: PresentationTarget::Browser { .. },
            ..
        } => "present_browser",
        CardCommand::Present {
            target: PresentationTarget::Preview { .. },
            ..
        } => "present_preview",
        CardCommand::Ego { command, .. } => match command {
            norma::protocol::EgoCommand::Status => "ego_status",
            norma::protocol::EgoCommand::Running => "ego_running",
            norma::protocol::EgoCommand::Handoff { .. } => "ego_handoff",
            norma::protocol::EgoCommand::Complete => "ego_complete",
            norma::protocol::EgoCommand::Fail { .. } => "ego_fail",
        },
        _ => "non_managed",
    }
}

pub(super) fn agent_profile_label(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::Cursor => "cursor",
        AgentProfile::Grok => "grok",
        AgentProfile::OpenCode => "opencode",
        AgentProfile::InfCode => "infcode",
        AgentProfile::Kimi => "kimi",
        AgentProfile::ZCode => "zcode",
        AgentProfile::Pi => "pi",
        AgentProfile::Hermes => "hermes",
    }
}

pub(super) fn managed_error_code(error: &CardCommandError) -> &'static str {
    match error {
        CardCommandError::Agent(failure) => match failure.code {
            AgentErrorCode::AppUnavailable => "app_unavailable",
            AgentErrorCode::OrchestrationChecking => "orchestration_checking",
            AgentErrorCode::ProtocolVersionMismatch => "protocol_version_mismatch",
            AgentErrorCode::WorkspaceRequired => "workspace_required",
            AgentErrorCode::WorkspaceNotFound => "workspace_not_found",
            AgentErrorCode::WorkspaceNotOpen => "workspace_not_open",
            AgentErrorCode::WorkspaceRouteExpired => "workspace_route_expired",
            AgentErrorCode::AmbiguousWorkspace => "ambiguous_workspace",
            AgentErrorCode::WorkspaceIdentityConflict => "workspace_identity_conflict",
            AgentErrorCode::CrossWorkspaceForbidden => "cross_workspace_forbidden",
            AgentErrorCode::ClientUpgradeRequired => "client_upgrade_required",
            AgentErrorCode::ProjectUnavailable => "project_unavailable",
            AgentErrorCode::Unauthorized => "unauthorized",
            AgentErrorCode::ProviderNotInstalled => "provider_not_installed",
            AgentErrorCode::SourceAgentUnavailable => "source_agent_unavailable",
            AgentErrorCode::UnknownAgent => "unknown_agent",
            AgentErrorCode::AgentDisabled => "agent_disabled",
            AgentErrorCode::EmptyTask => "empty_task",
            AgentErrorCode::SelfDispatchForbidden => "self_dispatch_forbidden",
            AgentErrorCode::DelegationCycle => "delegation_cycle",
            AgentErrorCode::DelegationBudgetExceeded => "delegation_budget_exceeded",
            AgentErrorCode::UntrackedRequiresForce => "untracked_requires_force",
            AgentErrorCode::TaskNotFound => "task_not_found",
            AgentErrorCode::RuntimeExpired => "runtime_expired",
            AgentErrorCode::TaskUntracked => "task_untracked",
            AgentErrorCode::TaskResultNotReady => "task_result_not_ready",
            AgentErrorCode::TaskNotCancellable => "task_not_cancellable",
            AgentErrorCode::WaitTimeout => "wait_timeout",
            AgentErrorCode::IdempotencyConflict => "idempotency_conflict",
            AgentErrorCode::OutcomeUnknown => "outcome_unknown",
            AgentErrorCode::ServerOverloaded => "server_overloaded",
            AgentErrorCode::AppShuttingDown => "app_shutting_down",
            AgentErrorCode::LegacyTaskRouteUnavailable => "legacy_task_route_unavailable",
            AgentErrorCode::InstallConflict => "install_conflict",
            AgentErrorCode::ComponentInstallFailed => "component_install_failed",
            AgentErrorCode::UnsupportedProvider => "unsupported_provider",
            AgentErrorCode::InternalError => "internal_error",
        },
        CardCommandError::Presentation(failure) => match failure.code {
            PresentationErrorCode::InvalidUrl => "invalid_url",
            PresentationErrorCode::ForbiddenUrlScheme => "forbidden_url_scheme",
            PresentationErrorCode::PreviewNotFound => "preview_not_found",
            PresentationErrorCode::PreviewNotFile => "preview_not_file",
            PresentationErrorCode::PreviewUnreadable => "preview_unreadable",
            PresentationErrorCode::UnsupportedPreviewType => "unsupported_preview_type",
            PresentationErrorCode::PresentationFailed => "presentation_failed",
        },
        CardCommandError::Ego(failure) => match failure.code {
            EgoErrorCode::Busy => "busy",
            EgoErrorCode::Unauthorized => "unauthorized",
            EgoErrorCode::InvalidState => "invalid_state",
            EgoErrorCode::TaskSpaceMissing => "task_space_missing",
            EgoErrorCode::IntegrationUnavailable => "integration_unavailable",
            EgoErrorCode::InternalError => "internal_error",
        },
        _ => "internal_error",
    }
}

pub(crate) fn log_managed_request(
    request_id: u64,
    command: &CardCommand,
    authorization: Option<&ProviderAuthorization>,
    source_node_id: Option<NodeId>,
    result: &Result<CardCommandResult, CardCommandError>,
    elapsed: Duration,
) {
    let command_name = managed_command_label(command);
    let provider = authorization
        .map(|authorization| agent_profile_label(authorization.provider))
        .unwrap_or("missing");
    let error_code = result
        .as_ref()
        .err()
        .map(managed_error_code)
        .unwrap_or("none");
    if let CardCommand::Present { target, .. } = command {
        let diagnostics = norma::presentation::presentation_diagnostics(target);
        let (presented_node_id, disposition) = match result {
            Ok(CardCommandResult::Presented {
                node_id,
                disposition,
                ..
            }) => (
                *node_id,
                match disposition {
                    PresentationDisposition::Created => "created",
                    PresentationDisposition::Reused => "reused",
                },
            ),
            Ok(CardCommandResult::EgoPresented {
                status,
                disposition,
            }) => (
                status.node_id,
                match disposition {
                    EgoPresentationDisposition::Created => "created",
                    EgoPresentationDisposition::Reused => "reused",
                    EgoPresentationDisposition::Replaced => "replaced",
                    EgoPresentationDisposition::Focused => "focused",
                },
            ),
            _ => (0, "none"),
        };
        tracing::info!(
            target: "norma_presentation",
            request_id,
            command = command_name,
            provider,
            source_node_id = source_node_id.unwrap_or_default(),
            source_present = source_node_id.is_some(),
            target_scheme = diagnostics.scheme.as_deref().unwrap_or("none"),
            target_host = diagnostics.host.as_deref().unwrap_or("none"),
            target_file = diagnostics.file_name.as_deref().unwrap_or("none"),
            target_sha256 = diagnostics.target_sha256,
            presented_node_id,
            disposition,
            ok = result.is_ok(),
            error_code,
            elapsed_ms = elapsed.as_millis() as u64,
            "presentation request completed"
        );
    } else {
        tracing::info!(
            target: "norma_orchestration",
            request_id,
            command = command_name,
            provider,
            source_node_id = source_node_id.unwrap_or_default(),
            source_present = source_node_id.is_some(),
            ok = result.is_ok(),
            error_code,
            elapsed_ms = elapsed.as_millis() as u64,
            "orchestration request completed"
        );
    }
}
