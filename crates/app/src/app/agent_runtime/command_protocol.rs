use norma::protocol::{AgentErrorCode, AgentProfile, CardCommandError};

pub(super) fn tracking_label(tracking: agent::AgentTrackingMode) -> &'static str {
    match tracking {
        agent::AgentTrackingMode::Managed => "managed",
        agent::AgentTrackingMode::Untracked => "untracked",
    }
}

pub(super) fn profile_slug(profile: AgentProfile) -> &'static str {
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

pub(super) fn internal_agent_error(message: String) -> CardCommandError {
    CardCommandError::agent(AgentErrorCode::InternalError, message, false)
}

pub(super) fn dispatch_assignments(
    coordinator: &mut agent::AgentTaskCoordinator,
    assignments: Vec<agent::AgentAssignment>,
    force_untracked: bool,
    source: agent::AgentDispatchSource,
    identity: Option<&super::AgentIdentitySnapshot>,
) -> Result<Vec<agent::AgentDispatchReceipt>, norma::protocol::CardCommandError> {
    let has_active = coordinator.active_task_ref_for_source(source).is_some();
    coordinator
        .dispatch_from(assignments, force_untracked, source)
        .map_err(|error| {
            if matches!(error, agent::AgentCoordinationError::SourceAgentUnavailable) {
                tracing::warn!(
                    definition_id = source.definition_id,
                    node_id = source.node_id,
                    has_active,
                    "resolve_dispatch_parent failed"
                );
            }
            coordination_error(error, identity)
        })
}

pub(super) fn coordination_error(
    error: agent::AgentCoordinationError,
    identity: Option<&super::AgentIdentitySnapshot>,
) -> CardCommandError {
    if matches!(
        &error,
        agent::AgentCoordinationError::SelfDispatchForbidden { .. }
    ) {
        if let Some(identity) = identity {
            return super::self_dispatch_forbidden_error(identity);
        }
    }
    let code = match &error {
        agent::AgentCoordinationError::UnknownName(_) => AgentErrorCode::UnknownAgent,
        agent::AgentCoordinationError::Disabled(_) => AgentErrorCode::AgentDisabled,
        agent::AgentCoordinationError::EmptyTask(_) => AgentErrorCode::EmptyTask,
        agent::AgentCoordinationError::UntrackedRequiresForce(_) => {
            AgentErrorCode::UntrackedRequiresForce
        }
        agent::AgentCoordinationError::SourceAgentUnavailable => {
            AgentErrorCode::SourceAgentUnavailable
        }
        agent::AgentCoordinationError::SelfDispatchForbidden { .. } => {
            AgentErrorCode::SelfDispatchForbidden
        }
        agent::AgentCoordinationError::DelegationCycle { .. } => AgentErrorCode::DelegationCycle,
        agent::AgentCoordinationError::DelegationBudgetExceeded { .. } => {
            AgentErrorCode::DelegationBudgetExceeded
        }
        agent::AgentCoordinationError::InvalidName(_)
        | agent::AgentCoordinationError::DuplicateName(_)
        | agent::AgentCoordinationError::Inactive(_) => AgentErrorCode::InternalError,
    };
    CardCommandError::agent(code, error.to_string(), false)
}
