use super::super::CardIdentity;
use crate::mobile::TaskProgress;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Default)]
pub struct Snapshot {
    pub workspaces: Vec<Workspace>,
    pub tasks: Vec<Task>,
}
#[derive(Clone, Serialize)]
pub struct Workspace {
    pub workspace_id: String,
    pub name: String,
    pub active: bool,
    pub projects: Vec<Project>,
    pub cards: Vec<Card>,
}
#[derive(Clone, Serialize)]
pub struct Project {
    pub project_id: String,
    pub area_id: String,
    pub name: String,
}
#[derive(Clone, Serialize)]
pub struct Card {
    #[serde(skip)]
    pub identity: CardIdentity,
    pub card_id: String,
    pub card_revision: String,
    pub role_name: String,
    pub role_name_unique: bool,
    pub project_id: String,
    pub profile: String,
    pub state: String,
    pub current_task_id: Option<String>,
    pub loaded: bool,
}
#[derive(Clone, Serialize)]
pub struct Task {
    pub task_id: String,
    pub card_id: String,
    pub role_name: String,
    pub workspace_id: String,
    pub state: String,
    pub tracking: String,
    pub summary: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub source: String,
    #[serde(skip)]
    pub operation_id: Option<String>,
    #[serde(skip)]
    pub progress: Option<TaskProgress>,
    #[serde(skip)]
    pub pending_approval: Option<Value>,
    #[serde(skip)]
    pub result_summary: Option<String>,
    #[serde(skip)]
    pub error: Option<String>,
}
impl Task {
    pub fn terminal(&self) -> bool {
        matches!(
            self.state.as_str(),
            "done" | "failed" | "cancelled" | "interrupted"
        )
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Directory {
    pub workspace_id: Option<String>,
    #[serde(default = "yes")]
    pub include_idle: bool,
}
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListTasks {
    pub workspace_id: Option<String>,
    pub card_id: Option<String>,
    pub states: Option<Vec<String>>,
    pub since: Option<i64>,
    pub cursor: Option<String>,
    #[serde(default = "twenty")]
    pub limit: usize,
}
fn twenty() -> usize {
    20
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Progress {
    pub task_id: Option<String>,
    pub operation_id: Option<String>,
}
