use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceSummary {
    pub workspace_id: String,
    pub display_name: String,
    pub canonical_root: PathBuf,
    pub active: bool,
    pub route_state: String,
    pub agent_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrchestrationDoctorReport {
    pub app_version: String,
    pub cli_version: String,
    pub protocol_version: u32,
    pub socket: String,
    pub hook_ingress: String,
    pub provider_health: String,
    pub skill_health: String,
    pub open_workspace_count: usize,
}
