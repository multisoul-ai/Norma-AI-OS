//! Trusted entry provenance. Credentials remain private to the entry authorization adapter.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceOperationOrigin {
    Desktop { host_id: String },
    Mobile { device_id: String },
}

impl WorkspaceOperationOrigin {
    pub fn key(&self) -> String {
        match self {
            Self::Desktop { host_id } => format!("desktop:{host_id}"),
            Self::Mobile { device_id } => format!("mobile:{device_id}"),
        }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        let id = match self {
            Self::Desktop { host_id } => host_id,
            Self::Mobile { device_id } => device_id,
        };
        valid_id(id).then_some(()).ok_or("invalid_origin")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceOperationSource {
    pub workspace_id: String,
    pub origin: WorkspaceOperationOrigin,
    /// Created once when the user sends, never derived from a model tool_call_id.
    pub operation_id: String,
    pub message_id: String,
    pub original_text: String,
}

impl WorkspaceOperationSource {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.origin.validate()?;
        if [&self.workspace_id, &self.operation_id, &self.message_id]
            .into_iter()
            .any(|id| !valid_id(id))
        {
            return Err("invalid_source_identity");
        }
        if self.original_text.trim().is_empty()
            || self.original_text.len() > 64 * 1024
            || self
                .original_text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err("invalid_original_text");
        }
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
