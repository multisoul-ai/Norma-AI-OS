use super::{ConstraintEnforcement, ContextRef};
use serde::{Deserialize, Serialize};

/// A model proposal; only the host can bind it to the current authenticated user input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextStateUpdate {
    pub input: ContextRef,
    pub expected_revision: u64,
    pub change: ContextStateChange,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextStateChange {
    Goal {
        text: String,
        supersedes: Option<ContextRef>,
    },
    Constraint {
        key: String,
        text: String,
        enforcement: ConstraintEnforcement,
        supersedes: Option<ContextRef>,
    },
    RevokeConstraint {
        key: String,
        supersedes: ContextRef,
    },
}
