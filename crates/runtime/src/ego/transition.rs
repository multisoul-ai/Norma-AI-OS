use super::{EgoOwnership, EgoTaskCommand, EgoTaskError, EgoTaskState};
use canvas::{EgoCardPhase, NodeId};

pub(super) fn transition(
    current: EgoTaskState,
    command: EgoTaskCommand,
) -> Result<EgoTaskState, EgoTaskError> {
    match command {
        EgoTaskCommand::TaskSpaceReady {
            task_space_id: _,
            task_space_name: _,
        } if current.phase == EgoCardPhase::PreparingAgent => Ok(EgoTaskState {
            phase: EgoCardPhase::PreparingAgent,
            ownership: current.ownership,
            message: None,
            bound_agent_node_id: current.bound_agent_node_id,
        }),
        EgoTaskCommand::TaskSpaceReady {
            task_space_id: _,
            task_space_name: _,
        } if matches!(
            current.phase,
            EgoCardPhase::Creating | EgoCardPhase::Disconnected
        ) =>
        {
            Ok(EgoTaskState {
                phase: EgoCardPhase::ReadyForUser,
                ownership: EgoOwnership::Inactive,
                message: None,
                bound_agent_node_id: current.bound_agent_node_id,
            })
        }
        EgoTaskCommand::PrepareAgent { agent_node_id } => {
            if let Some(owner) = active_owner(&current) {
                if owner != agent_node_id {
                    return Err(EgoTaskError::Busy {
                        current_owner: owner,
                    });
                }
            }
            if let Some(expected_agent) = current.bound_agent_node_id {
                if expected_agent != agent_node_id {
                    return Err(EgoTaskError::Unauthorized {
                        expected_agent,
                        actual_agent: agent_node_id,
                    });
                }
            }
            if !matches!(
                current.phase,
                EgoCardPhase::Creating
                    | EgoCardPhase::ReadyForUser
                    | EgoCardPhase::PreparingAgent
                    | EgoCardPhase::Completed
                    | EgoCardPhase::Failed
                    | EgoCardPhase::Disconnected
            ) {
                return Err(invalid(&current, "prepare_agent"));
            }
            Ok(EgoTaskState {
                phase: EgoCardPhase::PreparingAgent,
                ownership: EgoOwnership::Agent {
                    node_id: agent_node_id,
                },
                message: None,
                bound_agent_node_id: Some(agent_node_id),
            })
        }
        EgoTaskCommand::AgentRunning { agent_node_id }
            if matches!(
                current.phase,
                EgoCardPhase::PreparingAgent | EgoCardPhase::ResumeRequested
            ) =>
        {
            ensure_bound_agent(&current, agent_node_id)?;
            Ok(EgoTaskState {
                phase: EgoCardPhase::AgentWorking,
                ownership: EgoOwnership::Agent {
                    node_id: agent_node_id,
                },
                message: None,
                bound_agent_node_id: Some(agent_node_id),
            })
        }
        EgoTaskCommand::AgentHandoff {
            agent_node_id,
            message,
        } if current.phase == EgoCardPhase::AgentWorking => {
            ensure_bound_agent(&current, agent_node_id)?;
            Ok(EgoTaskState {
                phase: EgoCardPhase::AwaitingUser,
                ownership: EgoOwnership::AgentDelegatedToUser {
                    node_id: agent_node_id,
                },
                message: Some(message),
                bound_agent_node_id: Some(agent_node_id),
            })
        }
        EgoTaskCommand::UserTakeover if current.phase == EgoCardPhase::AwaitingUser => {
            Ok(EgoTaskState {
                phase: EgoCardPhase::UserControlling,
                ownership: EgoOwnership::User,
                message: current.message,
                bound_agent_node_id: current.bound_agent_node_id,
            })
        }
        EgoTaskCommand::UserReturn if current.phase == EgoCardPhase::UserControlling => {
            Ok(EgoTaskState {
                phase: EgoCardPhase::ResumeRequested,
                ownership: EgoOwnership::User,
                message: current.message,
                bound_agent_node_id: current.bound_agent_node_id,
            })
        }
        EgoTaskCommand::Complete { agent_node_id }
            if current.phase == EgoCardPhase::AgentWorking =>
        {
            ensure_bound_agent(&current, agent_node_id)?;
            Ok(EgoTaskState {
                phase: EgoCardPhase::Completed,
                ownership: EgoOwnership::Inactive,
                message: None,
                bound_agent_node_id: Some(agent_node_id),
            })
        }
        EgoTaskCommand::Fail {
            agent_node_id,
            message,
        } if matches!(
            current.phase,
            EgoCardPhase::PreparingAgent
                | EgoCardPhase::AgentWorking
                | EgoCardPhase::ResumeRequested
        ) =>
        {
            ensure_bound_agent(&current, agent_node_id)?;
            Ok(EgoTaskState {
                phase: EgoCardPhase::Failed,
                ownership: EgoOwnership::Inactive,
                message: Some(message),
                bound_agent_node_id: Some(agent_node_id),
            })
        }
        EgoTaskCommand::Disconnect { message } => Ok(EgoTaskState {
            phase: EgoCardPhase::Disconnected,
            ownership: EgoOwnership::Unknown,
            message: Some(message),
            bound_agent_node_id: current.bound_agent_node_id,
        }),
        command => Err(invalid(&current, command.operation())),
    }
}

pub(super) fn active_owner(state: &EgoTaskState) -> Option<NodeId> {
    match state.ownership {
        EgoOwnership::Agent { node_id } | EgoOwnership::AgentDelegatedToUser { node_id } => {
            Some(node_id)
        }
        EgoOwnership::User | EgoOwnership::Inactive | EgoOwnership::Unknown => {
            state.bound_agent_node_id.filter(|_| {
                matches!(
                    state.phase,
                    EgoCardPhase::UserControlling | EgoCardPhase::ResumeRequested
                )
            })
        }
    }
}

fn ensure_bound_agent(state: &EgoTaskState, actual_agent: NodeId) -> Result<(), EgoTaskError> {
    match state.bound_agent_node_id {
        Some(expected_agent) if expected_agent == actual_agent => Ok(()),
        Some(expected_agent) => Err(EgoTaskError::Unauthorized {
            expected_agent,
            actual_agent,
        }),
        None => Err(EgoTaskError::InvalidTransition {
            phase: state.phase,
            operation: "agent_without_binding",
        }),
    }
}

fn invalid(state: &EgoTaskState, operation: &'static str) -> EgoTaskError {
    EgoTaskError::InvalidTransition {
        phase: state.phase,
        operation,
    }
}
