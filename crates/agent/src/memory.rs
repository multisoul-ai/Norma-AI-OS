//! Pure memory protocol. Scope identities and provenance are supplied by the host,
//! never interpreted as filesystem paths or granted to a model by this module.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemoryScope {
    Personal,
    ProjectShared {
        project_id: String,
    },
    Worktree {
        project_id: String,
        worktree_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MemoryEntryId(String);

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error(
    "memory entry ID must be 1–128 lowercase ASCII letters, digits, '_' or '-', excluding 'memory'"
)]
pub struct MemoryEntryIdError;

impl MemoryEntryId {
    pub fn parse(value: &str) -> Result<Self, MemoryEntryIdError> {
        // 128 ASCII bytes + '.md' stays below the platform's 255-byte filename limit.
        if !(1..=128).contains(&value.len())
            || value == "memory"
            || !value
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
        {
            return Err(MemoryEntryIdError);
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for MemoryEntryId {
    type Error = MemoryEntryIdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<MemoryEntryId> for String {
    fn from(value: MemoryEntryId) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoryRevision(u64);

impl MemoryRevision {
    pub const INITIAL: Self = Self(0);
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    pub const fn number(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    User,
    Feedback,
    Project,
    Reference,
}

/// References, not evidence verification. The app binds these to real recorded
/// turns/receipts or settings edits when conversation tools are connected.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MemorySourceReference {
    UserTurn {
        session_id: String,
        turn_id: String,
    },
    ToolReceipt {
        turn_id: String,
        tool_call_id: String,
    },
    SettingsEdit {
        edit_id: String,
    },
}

impl MemorySourceReference {
    pub fn is_well_formed(&self) -> bool {
        let valid = |id: &str| !id.trim().is_empty() && !id.chars().any(char::is_control);
        match self {
            Self::UserTurn {
                session_id,
                turn_id,
            } => valid(session_id) && valid(turn_id),
            Self::ToolReceipt {
                turn_id,
                tool_call_id,
            } => valid(turn_id) && valid(tool_call_id),
            Self::SettingsEdit { edit_id } => valid(edit_id),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemorySummary {
    pub id: MemoryEntryId,
    pub name: String,
    pub description: String,
    pub kind: Option<MemoryKind>,
    pub modified: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryEntry {
    pub summary: MemorySummary,
    pub markdown: String,
    pub sources: Vec<MemorySourceReference>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryMutation {
    Upsert {
        id: MemoryEntryId,
        markdown: String,
        sources: Vec<MemorySourceReference>,
    },
    Delete {
        id: MemoryEntryId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryCommitReceipt {
    pub revision: MemoryRevision,
    pub changed_entries: Vec<MemoryEntryId>,
}

#[cfg(test)]
#[path = "../tests/unit/memory_tests.rs"]
mod tests;
