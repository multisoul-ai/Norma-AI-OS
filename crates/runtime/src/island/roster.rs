//! Compact slots, width, moods and the pending queue. Pure data: no GPUI, no clocks.
use super::bloub::Mood;

/// How long a completion stays open before the island collapses. The countdown bar uses the same window.
pub const ACTIVITY_HOLD_MS: u64 = 3_000;

/// One agent, already classified. The same id must not occupy both sides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotAgent {
    pub id: String,
    pub slot: SlotKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    /// Left side of the compact island.
    Running,
    /// Right side. Approvals, questions and reviews share this side.
    Pending,
    /// Queued or finished with nothing to review. Not shown in either slot.
    Hidden,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactRoster {
    pub running: Vec<String>,
    pub running_overflow: usize,
    pub pending: Vec<String>,
    pub pending_total: usize,
}

impl CompactRoster {
    pub fn running_total(&self) -> usize {
        self.running.len() + self.running_overflow
    }
}

/// What the face should do. Separate from which compact slot it occupies:
/// a finished review sits on the right but wears the responded face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentActivity {
    Running,
    Waiting,
    Done,
    Queued,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AvatarFace {
    pub mood: Mood,
    pub opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunFilter {
    All,
    Running,
    Waiting,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilterCounts {
    pub all: usize,
    pub running: usize,
    pub waiting: usize,
    pub done: usize,
}

/// Left slot shows at most three running agents. Right slot shows at most two pending agents.
/// An id listed as both running and pending is pending only, in the order pending rows appear.
pub fn compact_roster(agents: &[SlotAgent]) -> CompactRoster {
    let mut pending = Vec::new();
    let mut claimed = std::collections::BTreeSet::new();
    for agent in agents
        .iter()
        .filter(|agent| agent.slot == SlotKind::Pending)
    {
        if claimed.insert(agent.id.clone()) {
            pending.push(agent.id.clone());
        }
    }
    let mut running = Vec::new();
    for agent in agents
        .iter()
        .filter(|agent| agent.slot == SlotKind::Running)
    {
        if claimed.contains(&agent.id) || !claimed.insert(agent.id.clone()) {
            continue;
        }
        running.push(agent.id.clone());
    }
    let running_total = running.len();
    let pending_total = pending.len();
    CompactRoster {
        running: running.into_iter().take(3).collect(),
        running_overflow: running_total.saturating_sub(3),
        pending: pending.into_iter().take(2).collect(),
        pending_total,
    }
}

/// Running count plus pending count. Four or more widens the capsule from 330 to 416.
pub fn needs_wide_capsule(running_total: usize, pending_total: usize) -> bool {
    running_total.saturating_add(pending_total) >= 4
}

/// Width of the expanded running-tab face strip. Each face is 20pt and the next overlaps by 2pt.
pub fn overview_strip_width(count: usize) -> f32 {
    if count == 0 {
        0.
    } else {
        20. + (count - 1) as f32 * 18.
    }
}

pub fn avatar_face(activity: AgentActivity) -> AvatarFace {
    match activity {
        AgentActivity::Running => AvatarFace {
            mood: Mood::Working,
            opacity: 1.,
        },
        AgentActivity::Waiting => AvatarFace {
            mood: Mood::Waiting,
            opacity: 1.,
        },
        AgentActivity::Done => AvatarFace {
            mood: Mood::Responded,
            opacity: 1.,
        },
        AgentActivity::Queued => AvatarFace {
            mood: Mood::Idle,
            opacity: 0.55,
        },
    }
}

pub fn accepts(filter: RunFilter, activity: AgentActivity) -> bool {
    match filter {
        RunFilter::All => true,
        RunFilter::Running => activity == AgentActivity::Running,
        RunFilter::Waiting => activity == AgentActivity::Waiting,
        RunFilter::Done => activity == AgentActivity::Done,
    }
}

pub fn filter_counts(activities: &[AgentActivity]) -> FilterCounts {
    FilterCounts {
        all: activities.len(),
        running: activities
            .iter()
            .filter(|activity| accepts(RunFilter::Running, **activity))
            .count(),
        waiting: activities
            .iter()
            .filter(|activity| accepts(RunFilter::Waiting, **activity))
            .count(),
        done: activities
            .iter()
            .filter(|activity| accepts(RunFilter::Done, **activity))
            .count(),
    }
}

/// The queue is visible once two or more items need the user.
pub fn pending_queue_visible(pending_total: usize) -> bool {
    pending_total >= 2
}

/// After the current card leaves the list, show the following remaining id.
/// The last card falls back to the previous one. An unknown current id uses the first remaining.
pub fn pending_successor(
    previous: &[String],
    remaining: &[String],
    current: &str,
) -> Option<String> {
    if remaining.iter().any(|id| id == current) {
        return Some(current.to_owned());
    }
    let Some(index) = previous.iter().position(|id| id == current) else {
        return remaining.first().cloned();
    };
    previous
        .iter()
        .skip(index + 1)
        .find(|id| remaining.iter().any(|item| item == *id))
        .or_else(|| {
            previous
                .iter()
                .take(index)
                .rev()
                .find(|id| remaining.iter().any(|item| item == *id))
        })
        .cloned()
        .or_else(|| remaining.first().cloned())
}

/// Width of the green countdown, 1 at the start and 0 at the deadline.
/// Reduce Motion keeps the line full so it does not animate.
pub fn countdown_scale(now_ms: u64, deadline_ms: u64, reduce_motion: bool) -> f64 {
    if reduce_motion {
        return 1.;
    }
    if now_ms >= deadline_ms {
        return 0.;
    }
    let start = deadline_ms.saturating_sub(ACTIVITY_HOLD_MS);
    if now_ms <= start {
        return 1.;
    }
    (deadline_ms - now_ms) as f64 / ACTIVITY_HOLD_MS as f64
}

#[cfg(test)]
#[path = "../../tests/unit/island_roster.rs"]
mod tests;
