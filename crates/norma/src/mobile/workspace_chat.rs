//! Desktop workspace identity is separate from the legacy project `workspace_id`.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMessageIntent {
    #[default]
    Discuss,
    Execute,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceAgentTarget {
    pub area_id: String,
    pub definition_id: i64,
    pub node_id: u64,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    pub task_ref: Option<String>,
}

impl WorkspaceAgentTarget {
    pub fn validate_card(
        &self,
        area: &str,
        definition: i64,
        card: &super::CardSnapshot,
    ) -> Result<(), &'static str> {
        if self.area_id != area || self.node_id != card.node_id || self.definition_id != definition
        {
            return Err("target_missing");
        }
        if self.run_id != card.run_id || self.session_id != card.session_id {
            return Err("session_changed");
        }
        if self.task_ref.is_some() && (self.run_id.is_none() || self.session_id.is_none()) {
            return Err("task_expired");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceChatMessage {
    pub desktop_workspace_id: String,
    pub operation_id: String,
    pub message_id: String,
    pub text: String,
    #[serde(default)]
    pub intent: WorkspaceMessageIntent,
    pub target: Option<WorkspaceAgentTarget>,
    pub idea_id: Option<String>,
}

impl WorkspaceChatMessage {
    pub fn validate(&self) -> Result<()> {
        for id in [
            &self.desktop_workspace_id,
            &self.operation_id,
            &self.message_id,
        ] {
            validate_id(id)?;
        }
        ensure!(
            !self.text.trim().is_empty() && self.text.len() <= 64 * 1024,
            "invalid_message"
        );
        if let Some(target) = &self.target {
            validate_id(&target.area_id)?;
            ensure!(
                target.definition_id > 0 && target.node_id > 0,
                "invalid_target"
            );
            if let Some(task) = &target.task_ref {
                ensure!(!task.is_empty() && task.len() <= 256, "invalid_task_ref");
            }
        }
        if let Some(id) = &self.idea_id {
            validate_id(id)?;
        }
        Ok(())
    }
}

pub fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "invalid_id"
    );
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DesktopChatWorkspace {
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub projects: Vec<DesktopChatProject>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DesktopChatProject {
    pub area_id: String,
    pub project_id: String,
    pub name: String,
    pub root: String,
    pub agents: Vec<DesktopChatAgent>,
    pub definitions: Vec<DesktopChatDefinition>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DesktopChatDefinition {
    pub id: i64,
    pub name: String,
    pub profile: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DesktopChatAgent {
    pub definition_id: i64,
    pub name: String,
    pub profile: String,
    pub card: super::CardSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceConfigurationChange {
    Create { name: String },
    Rename { name: String },
    AddProject { project_id: String },
    RemoveProject { area_id: String },
    AddAgent { area_id: String, definition_id: i64 },
    RemoveAgent { area_id: String, node_id: u64 },
}
