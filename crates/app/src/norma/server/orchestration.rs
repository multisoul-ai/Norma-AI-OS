use crate::app::MultisoulApp;
use canvas::{AgentProfile as CanvasAgentProfile, CanvasNodeKind};
use norma::protocol::{
    AgentErrorCode, AgentProfile, CardCommand, CardCommandError, NodeId, ProviderAuthorization,
};

pub(super) fn is_orchestration_command(command: &CardCommand) -> bool {
    matches!(
        command,
        CardCommand::ListAgents
            | CardCommand::DispatchAgentTasks { .. }
            | CardCommand::AgentTaskStatus { .. }
            | CardCommand::AgentTaskCancel { .. }
    )
}

impl MultisoulApp {
    pub(super) fn validate_orchestration_source(
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
    }
}

fn norma_agent_profile_from_canvas(profile: CanvasAgentProfile) -> AgentProfile {
    match profile {
        CanvasAgentProfile::Claude => AgentProfile::Claude,
        CanvasAgentProfile::Codex => AgentProfile::Codex,
        CanvasAgentProfile::Cursor => AgentProfile::Cursor,
        CanvasAgentProfile::Grok => AgentProfile::Grok,
        CanvasAgentProfile::OpenCode => AgentProfile::OpenCode,
    }
}

pub(super) fn orchestration_command_label(command: &CardCommand) -> &'static str {
    match command {
        CardCommand::ListAgents => "agent_list",
        CardCommand::DispatchAgentTasks { .. } => "agent_task_assign",
        CardCommand::AgentTaskStatus { .. } => "agent_task_status",
        CardCommand::AgentTaskCancel { .. } => "agent_task_cancel",
        _ => "non_orchestration",
    }
}

pub(super) fn agent_profile_label(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::Cursor => "cursor",
        AgentProfile::Grok => "grok",
        AgentProfile::OpenCode => "opencode",
    }
}

pub(super) fn orchestration_error_code(error: &CardCommandError) -> Option<&'static str> {
    let CardCommandError::Agent(failure) = error else {
        return Some("internal_error");
    };
    Some(match failure.code {
        AgentErrorCode::AppUnavailable => "app_unavailable",
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
        AgentErrorCode::WaitTimeout => "wait_timeout",
        AgentErrorCode::InstallConflict => "install_conflict",
        AgentErrorCode::ComponentInstallFailed => "component_install_failed",
        AgentErrorCode::UnsupportedProvider => "unsupported_provider",
        AgentErrorCode::InternalError => "internal_error",
    })
}
