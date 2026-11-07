//! A confirmation preference can belong to one workspace or to the whole App.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct AskRule {
    /// None preserves an existing App-wide preference, including future workspaces.
    pub workspace_id: Option<String>,
    pub key: String,
}

impl AskRule {
    pub fn matches(&self, workspace_id: Option<&str>, key: &str) -> bool {
        self.key == key
            && self
                .workspace_id
                .as_deref()
                .is_none_or(|scope| Some(scope) == workspace_id)
    }
}

impl From<String> for AskRule {
    fn from(key: String) -> Self {
        Self {
            workspace_id: None,
            key,
        }
    }
}
impl From<&str> for AskRule {
    fn from(key: &str) -> Self {
        key.to_owned().into()
    }
}
