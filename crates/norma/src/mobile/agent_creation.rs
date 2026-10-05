use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceAgentCreateRequest {
    pub desktop_workspace_id: String,
    pub area_id: String,
    pub operation_id: String,
    pub expected_revision: u64,
    pub profile: String,
}
impl WorkspaceAgentCreateRequest {
    pub fn validate(&self) -> Result<()> {
        for id in [
            &self.desktop_workspace_id,
            &self.area_id,
            &self.operation_id,
        ] {
            super::workspace_chat::validate_id(id)?;
        }
        ensure!(self.expected_revision > 0, "invalid_revision");
        ensure!(
            matches!(self.profile.as_str(), "Codex" | "Claude" | "Cursor"),
            "unsupported_profile"
        );
        Ok(())
    }
}
