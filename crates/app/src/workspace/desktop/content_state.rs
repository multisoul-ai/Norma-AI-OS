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
}

#[derive(serde::Deserialize, serde::Serialize)]
pub(crate) struct DesktopArtifactVersion {
    pub(crate) node_id: canvas::NodeId,
    pub(crate) fingerprint: String,
    pub(crate) source_path: String,
}
