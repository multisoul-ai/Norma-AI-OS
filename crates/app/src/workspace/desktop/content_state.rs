//! Identity of frozen task artifacts displayed in workspace content tabs.
use std::path::PathBuf;

#[derive(Clone)]
pub(crate) struct DesktopArtifact {
    pub(crate) area_id: String,
    pub(crate) node_id: canvas::NodeId,
    pub(crate) agent_profile: String,
    pub(crate) turn_id: String,
    pub(crate) path: PathBuf,
    pub(crate) source_path: String,
    pub(crate) version: String,
    pub(crate) saved_at_unix: u64,
    pub(crate) viewed: bool,
}

impl DesktopArtifact {
    pub(super) fn read_key(&self, workspace_id: &str) -> String {
        let identity = serde_json::to_string(&(
            workspace_id,
            &self.area_id,
            self.node_id,
            &self.version,
            &self.source_path,
        ))
        .expect("artifact identity must serialize");
        format!("desktop.artifact.read.v1:{identity}")
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub(crate) struct DesktopArtifactVersion {
    pub(crate) node_id: canvas::NodeId,
    pub(crate) fingerprint: String,
    pub(crate) source_path: String,
}
