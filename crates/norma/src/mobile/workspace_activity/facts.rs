use super::super::{ApprovalSnapshot, DesktopChatWorkspace};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActivityApprovalFact {
    pub area_id: String,
    pub approval: ApprovalSnapshot,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActivityReviewFact {
    /// Persisted delivery creation time, in unix seconds; zero means unavailable on older facts.
    #[serde(default)]
    pub saved_at_unix: u64,
    pub area_id: String,
    pub node_id: u64,
    pub profile: String,
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub fingerprint: String,
    pub completed: bool,
    pub available: bool,
    pub conflicted: bool,
    pub accepted: bool,
    pub file_count: usize,
}
impl ActivityReviewFact {
    pub(super) fn is_pending(&self) -> bool {
        self.completed && self.available && !self.conflicted && !self.accepted
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkspaceActivityFacts {
    pub workspace: DesktopChatWorkspace,
    pub observed: bool,
    pub approvals: Vec<ActivityApprovalFact>,
    pub reviews: Vec<ActivityReviewFact>,
}
