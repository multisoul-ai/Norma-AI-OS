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
        AgentProfile::Kimi => "kimi",
        AgentProfile::ZCode => "zcode",
        AgentProfile::Pi => "pi",
        AgentProfile::Hermes => "hermes",
    }
}

pub(super) fn internal_agent_error(message: String) -> CardCommandError {
    CardCommandError::agent(AgentErrorCode::InternalError, message, false)
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
