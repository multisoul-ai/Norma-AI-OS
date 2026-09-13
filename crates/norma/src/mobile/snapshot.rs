use serde::{Deserialize, Serialize};

/// Display metadata only; full conversation bodies and Provider credentials never enter this DTO.
#[derive(Clone, Debug, Deserialize, Serialize, Eq)]
pub struct CardSnapshot {
    pub node_id: u64,
    pub profile: String,
    pub title: String,
    pub mobile_control: bool,
    pub state: String,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    #[serde(default)]
    pub turn_id: Option<String>,
    pub task_title: Option<String>,
    /// Display capability only. None preserves older clients; the Runtime always checks the actual session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can_interrupt: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_timing: Option<TaskTiming>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_progress: Option<TaskProgress>,
    /// Bounded excerpt from this turn's observed activity/reply. Not a full conversation body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_summary: Option<String>,
}

impl PartialEq for CardSnapshot {
    fn eq(&self, other: &Self) -> bool {
        // Capabilities are advisory and may be omitted by older clients when echoing a target.
        self.node_id == other.node_id
            && self.profile == other.profile
            && self.title == other.title
            && self.mobile_control == other.mobile_control
            && self.state == other.state
            && self.run_id == other.run_id
            && self.session_id == other.session_id
            && self.turn_id == other.turn_id
            && self.task_title == other.task_title
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub workspace_id: String,
    pub display_name: String,
    /// open, closed, or unavailable; open means a loaded project session, with or without a window.
    pub state: String,
    pub route_id: Option<String>,
    pub card_count: Option<usize>,
    /// None is not an empty project: cards were not requested, or the project session is unavailable.
    pub cards: Option<Vec<CardSnapshot>>,
}

/// Measured from accepted Provider turn events. Stable while running; clients render elapsed time locally.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskTiming {
    pub started_at_unix_ms: u64,
    /// Monotonic duration, including tool execution and approval waits. None means still running.
    pub completed_elapsed_ms: Option<u64>,
}

impl CardSnapshot {
    pub(crate) fn same_display(&self, other: &Self) -> bool {
        self == other
            && self.can_interrupt == other.can_interrupt
            && self.task_timing == other.task_timing
            && self.task_progress == other.task_progress
            && self.task_summary == other.task_summary
    }
}

/// Counts only confirmed plan tools observed in the current turn, never tool-call counts.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaskProgress {
    pub completed: u32,
    pub total: u32,
    pub current_step: Option<String>,
}

impl ProjectSnapshot {
    pub(crate) fn same_display(&self, other: &Self) -> bool {
        self == other
            && match (&self.cards, &other.cards) {
                (Some(a), Some(b)) => {
                    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.same_display(b))
                }
                (None, None) => true,
                _ => false,
            }
    }
}
