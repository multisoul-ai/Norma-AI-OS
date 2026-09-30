use super::{CardSnapshot, ProjectSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectEvent {
    pub stream_id: String,
    pub workspace_id: String,
    pub sequence: u64,
    #[serde(flatten)]
    pub change: ProjectChange,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectChange {
    Snapshot {
        project: ProjectSnapshot,
    },
    Delta {
        display_name: String,
        upserted: Vec<CardSnapshot>,
        removed: Vec<u64>,
        order: Vec<u64>,
    },
    Heartbeat,
}

/// A bounded live projection, not a task audit log. Each connection starts with a new full baseline;
/// coalesced state changes are ordered on that stream, and missing history requires a new snapshot.
pub struct ProjectProjection {
    stream_id: String,
    sequence: u64,
    project: ProjectSnapshot,
}
impl ProjectProjection {
    pub fn new(stream_id: String, project: ProjectSnapshot) -> Self {
        Self {
            stream_id,
            sequence: 0,
            project,
        }
    }
    fn event(&self, change: ProjectChange) -> ProjectEvent {
        ProjectEvent {
            stream_id: self.stream_id.clone(),
            workspace_id: self.project.workspace_id.clone(),
            sequence: self.sequence,
            change,
        }
    }
    pub fn snapshot(&self) -> ProjectEvent {
        self.event(ProjectChange::Snapshot {
            project: self.project.clone(),
        })
    }
    pub fn heartbeat(&self) -> ProjectEvent {
        self.event(ProjectChange::Heartbeat)
    }
    pub fn update(
        &mut self,
        project: ProjectSnapshot,
    ) -> Result<Option<ProjectEvent>, &'static str> {
        if project.workspace_id != self.project.workspace_id {
            return Err("invalid_project");
        }
        if project.same_display(&self.project) {
            return Ok(None);
        }
        let change = match (&self.project.cards, &project.cards) {
            (Some(before), Some(after))
                if self.project.state == "open"
                    && project.state == "open"
                    && self.project.route_id == project.route_id =>
            {
                let before: BTreeMap<_, _> = before.iter().map(|c| (c.node_id, c)).collect();
                let after_map: BTreeMap<_, _> = after.iter().map(|c| (c.node_id, c)).collect();
                ProjectChange::Delta {
                    display_name: project.display_name.clone(),
                    upserted: after
                        .iter()
                        .filter(|c| {
                            before
                                .get(&c.node_id)
                                .is_none_or(|old| !old.same_display(c))
                        })
                        .cloned()
                        .collect(),
                    removed: before
                        .keys()
                        .filter(|id| !after_map.contains_key(id))
                        .copied()
                        .collect(),
                    order: after.iter().map(|c| c.node_id).collect(),
                }
            }
            _ => ProjectChange::Snapshot {
                project: project.clone(),
            },
        };
        self.sequence = self.sequence.checked_add(1).ok_or("stream_exhausted")?;
        self.project = project;
        Ok(Some(self.event(change)))
    }
}
