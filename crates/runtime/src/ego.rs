use crate::runtime::Runtime;
use canvas::{
    BrowserSpec, BrowserSurfaceKind, CanvasNodeKind, CanvasRect, EgoCardPhase, EgoLiteSpec, NodeId,
};
use std::collections::HashMap;
use std::fmt;

mod transition;
use transition::{active_owner, transition};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EgoOwnership {
    Agent { node_id: NodeId },
    AgentDelegatedToUser { node_id: NodeId },
    User,
    Inactive,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EgoTaskState {
    pub phase: EgoCardPhase,
    pub ownership: EgoOwnership,
    pub message: Option<String>,
    pub bound_agent_node_id: Option<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EgoTaskSpaceReplacement {
    previous_state: EgoTaskState,
    previous_task_space_id: Option<String>,
    previous_task_space_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EgoTaskCommand {
    TaskSpaceReady {
        task_space_id: String,
        task_space_name: String,
    },
    PrepareAgent {
        agent_node_id: NodeId,
    },
    AgentRunning {
        agent_node_id: NodeId,
    },
    AgentHandoff {
        agent_node_id: NodeId,
        message: String,
    },
    UserTakeover,
    UserReturn,
    Complete {
        agent_node_id: NodeId,
    },
    Fail {
        agent_node_id: NodeId,
        message: String,
    },
    Disconnect {
        message: String,
    },
}

impl EgoTaskCommand {
    fn operation(&self) -> &'static str {
        match self {
            Self::TaskSpaceReady { .. } => "task_space_ready",
            Self::PrepareAgent { .. } => "prepare_agent",
            Self::AgentRunning { .. } => "agent_running",
            Self::AgentHandoff { .. } => "agent_handoff",
            Self::UserTakeover => "user_takeover",
            Self::UserReturn => "user_return",
            Self::Complete { .. } => "complete",
            Self::Fail { .. } => "fail",
            Self::Disconnect { .. } => "disconnect",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EgoTaskError {
    MissingNode {
        node_id: NodeId,
    },
    NotEgoNode {
        node_id: NodeId,
    },
    Busy {
        current_owner: NodeId,
    },
    Unauthorized {
        expected_agent: NodeId,
        actual_agent: NodeId,
    },
    InvalidTransition {
        phase: EgoCardPhase,
        operation: &'static str,
    },
}

impl fmt::Display for EgoTaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingNode { node_id } => write!(formatter, "Ego card {node_id} was not found"),
            Self::NotEgoNode { node_id } => write!(formatter, "card {node_id} is not an Ego card"),
            Self::Busy { current_owner } => {
                write!(formatter, "Ego card is busy with agent {current_owner}")
            }
            Self::Unauthorized {
                expected_agent,
                actual_agent,
            } => write!(
                formatter,
                "Ego card belongs to agent {expected_agent}, not agent {actual_agent}"
            ),
            Self::InvalidTransition { phase, operation } => {
                write!(formatter, "cannot {operation} while Ego card is {phase:?}")
            }
        }
    }
}

impl std::error::Error for EgoTaskError {}

pub(crate) fn restored_ego_states(runtime: &Runtime) -> HashMap<NodeId, EgoTaskState> {
    runtime
        .document
        .nodes()
        .iter()
        .filter_map(|node| {
            let CanvasNodeKind::Browser(BrowserSpec {
                surface: BrowserSurfaceKind::EgoLite(spec),
                ..
            }) = &node.kind
            else {
                return None;
            };
            let has_task_space = spec.task_space_id.is_some();
            Some((
                node.id,
                EgoTaskState {
                    phase: if has_task_space {
                        EgoCardPhase::Disconnected
                    } else {
                        spec.last_phase
                    },
                    ownership: if has_task_space {
                        EgoOwnership::Unknown
                    } else {
                        EgoOwnership::Inactive
                    },
                    message: spec.last_message.clone(),
                    bound_agent_node_id: spec.bound_agent_node_id,
                },
            ))
        })
        .collect()
}

impl Runtime {
    pub fn spawn_ego_browser(
        &mut self,
        task_space_name: Option<String>,
        bounds: CanvasRect,
    ) -> NodeId {
        let spec = EgoLiteSpec {
            task_space_name,
            last_phase: EgoCardPhase::Creating,
            ..EgoLiteSpec::default()
        };
        let node_id = self.document.add_node(
            CanvasNodeKind::Browser(BrowserSpec {
                url: String::new(),
                title: None,
                surface: BrowserSurfaceKind::EgoLite(spec),
            }),
            bounds,
        );
        let state = EgoTaskState {
            phase: EgoCardPhase::Creating,
            ownership: EgoOwnership::Inactive,
            message: None,
            bound_agent_node_id: None,
        };
        self.ego_states.insert(node_id, state.clone());
        self.events
            .ego_task_state_changed(node_id, None, state, "create");
        node_id
    }

    pub fn ego_task_state(&self, node_id: NodeId) -> Option<EgoTaskState> {
        self.ego_states.get(&node_id).cloned()
    }

    pub fn ego_node_id(&self) -> Option<NodeId> {
        self.ego_states.keys().copied().min()
    }

    pub fn can_close_ego_node(&self, node_id: NodeId) -> bool {
        self.ego_states.get(&node_id).is_some_and(|state| {
            matches!(
                state.phase,
                EgoCardPhase::ReadyForUser
                    | EgoCardPhase::Completed
                    | EgoCardPhase::Failed
                    | EgoCardPhase::Disconnected
            )
        })
    }

    pub fn replace_ego_task_space(
        &mut self,
        node_id: NodeId,
    ) -> Result<EgoTaskState, EgoTaskError> {
        let current = self
            .ego_states
            .get(&node_id)
            .cloned()
            .ok_or_else(|| self.classify_missing_ego_node(node_id))?;
        // 卡片可关闭不等于可被 `present ego --new` 覆盖：后者只允许完成/失败的终态。
        if !matches!(
            current.phase,
            EgoCardPhase::Completed | EgoCardPhase::Failed
        ) {
            if let Some(current_owner) = active_owner(&current).or(current.bound_agent_node_id) {
                return Err(EgoTaskError::Busy { current_owner });
            }
            return Err(EgoTaskError::InvalidTransition {
                phase: current.phase,
                operation: "replace_task_space",
            });
        }

        let next = EgoTaskState {
            phase: EgoCardPhase::Creating,
            ownership: EgoOwnership::Inactive,
            message: None,
            bound_agent_node_id: None,
        };
        let previous_spec = self.runtime_ego_spec(node_id)?;
        self.ego_replacements.insert(
            node_id,
            EgoTaskSpaceReplacement {
                previous_state: current.clone(),
                previous_task_space_id: previous_spec.task_space_id.clone(),
                previous_task_space_name: previous_spec.task_space_name.clone(),
            },
        );
        self.persist_ego_state(node_id, &next)?;
        self.ego_states.insert(node_id, next.clone());
        self.events.ego_task_state_changed(
            node_id,
            Some(current),
            next.clone(),
            "replace_task_space",
        );
        Ok(next)
    }

    pub fn fail_ego_task_space_replacement(
        &mut self,
        node_id: NodeId,
        previous_task_space_is_still_available: bool,
        message: String,
    ) -> Result<EgoTaskState, EgoTaskError> {
        let current = self
            .ego_states
            .get(&node_id)
            .cloned()
            .ok_or_else(|| self.classify_missing_ego_node(node_id))?;
        let replacement =
            self.ego_replacements
                .remove(&node_id)
                .ok_or(EgoTaskError::InvalidTransition {
                    phase: current.phase,
                    operation: "fail_replace_task_space",
                })?;
        let next = EgoTaskState {
            phase: EgoCardPhase::Disconnected,
            ownership: EgoOwnership::Unknown,
            message: Some(message),
            bound_agent_node_id: previous_task_space_is_still_available
                .then_some(replacement.previous_state.bound_agent_node_id)
                .flatten(),
        };
        {
            let spec = self.runtime_ego_spec_mut(node_id)?;
            if previous_task_space_is_still_available {
                spec.task_space_id = replacement.previous_task_space_id;
                spec.task_space_name = replacement.previous_task_space_name;
            } else {
                spec.task_space_id = None;
                spec.task_space_name = None;
            }
        }
        self.persist_ego_state(node_id, &next)?;
        self.ego_states.insert(node_id, next.clone());
        self.events.ego_task_state_changed(
            node_id,
            Some(current),
            next.clone(),
            "replace_task_space_failed",
        );
        Ok(next)
    }

    pub fn ego_task_space_replacement_pending(&self, node_id: NodeId) -> bool {
        self.ego_replacements.contains_key(&node_id)
    }

    pub fn apply_ego_task_command(
        &mut self,
        node_id: NodeId,
        command: EgoTaskCommand,
    ) -> Result<EgoTaskState, EgoTaskError> {
        let operation = command.operation();
        let current = self
            .ego_states
            .get(&node_id)
            .cloned()
            .ok_or_else(|| self.classify_missing_ego_node(node_id))?;
        let task_space = match &command {
            EgoTaskCommand::TaskSpaceReady {
                task_space_id,
                task_space_name,
            } => Some((task_space_id.clone(), task_space_name.clone())),
            _ => None,
        };
        let next = transition(current.clone(), command)?;
        if let Some((task_space_id, task_space_name)) = task_space {
            let spec = self.runtime_ego_spec_mut(node_id)?;
            spec.task_space_id = Some(task_space_id);
            spec.task_space_name = Some(task_space_name);
            self.ego_replacements.remove(&node_id);
        }
        self.persist_ego_state(node_id, &next)?;
        self.ego_states.insert(node_id, next.clone());
        self.events
            .ego_task_state_changed(node_id, Some(current), next.clone(), operation);
        Ok(next)
    }

    pub(crate) fn remove_ego_state(&mut self, node_id: NodeId) {
        self.ego_states.remove(&node_id);
        self.ego_replacements.remove(&node_id);
    }

    pub(crate) fn restore_ego_states(&mut self) {
        self.ego_states = restored_ego_states(self);
        self.ego_replacements.clear();
    }

    fn classify_missing_ego_node(&self, node_id: NodeId) -> EgoTaskError {
        match self.runtime_ego_spec(node_id) {
            Ok(_) => EgoTaskError::MissingNode { node_id },
            Err(error) => error,
        }
    }

    fn persist_ego_state(
        &mut self,
        node_id: NodeId,
        state: &EgoTaskState,
    ) -> Result<(), EgoTaskError> {
        let spec = self.runtime_ego_spec_mut(node_id)?;
        spec.last_phase = state.phase;
        spec.last_message.clone_from(&state.message);
        spec.bound_agent_node_id = state.bound_agent_node_id;
        Ok(())
    }

    fn runtime_ego_spec(&self, node_id: NodeId) -> Result<&EgoLiteSpec, EgoTaskError> {
        let node = self
            .document
            .node(node_id)
            .ok_or(EgoTaskError::MissingNode { node_id })?;
        let CanvasNodeKind::Browser(BrowserSpec {
            surface: BrowserSurfaceKind::EgoLite(spec),
            ..
        }) = &node.kind
        else {
            return Err(EgoTaskError::NotEgoNode { node_id });
        };
        Ok(spec)
    }

    fn runtime_ego_spec_mut(&mut self, node_id: NodeId) -> Result<&mut EgoLiteSpec, EgoTaskError> {
        let node = self
            .document
            .node_mut(node_id)
            .ok_or(EgoTaskError::MissingNode { node_id })?;
        let CanvasNodeKind::Browser(BrowserSpec {
            surface: BrowserSurfaceKind::EgoLite(spec),
            ..
        }) = &mut node.kind
        else {
            return Err(EgoTaskError::NotEgoNode { node_id });
        };
        Ok(spec)
    }
}
