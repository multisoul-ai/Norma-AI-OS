use super::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum BrowserSurfaceKind {
    #[default]
    Native,
    EgoLite(EgoLiteSpec),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum EgoCardPhase {
    Creating,
    #[default]
    ReadyForUser,
    PreparingAgent,
    AgentWorking,
    AwaitingUser,
    UserControlling,
    ResumeRequested,
    Completed,
    Failed,
    Disconnected,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct EgoLiteSpec {
    #[serde(default)]
    pub task_space_id: Option<String>,
    #[serde(default)]
    pub task_space_name: Option<String>,
    #[serde(default)]
    pub bound_agent_node_id: Option<NodeId>,
    #[serde(default)]
    pub last_phase: EgoCardPhase,
    #[serde(default)]
    pub last_message: Option<String>,
}
