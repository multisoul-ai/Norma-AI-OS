use norma::{agent_hook::AgentHookEvent, mobile::TaskProgress, protocol::AgentProfile};
use serde_json::Value;
use std::collections::HashSet;

impl super::Runtime {
    pub fn mobile_task_progress(&self, node: canvas::NodeId) -> Option<TaskProgress> {
        self.mobile_runs.get(&node)?.task_steps.snapshot()
    }
}

#[derive(Default)]
pub(super) struct TaskSteps {
    turn: Option<String>,
    tasks: Vec<Step>,
    seen: HashSet<String>,
    incomplete: bool,
    /// Claude TodoWrite replaced the list. It cannot be merged with TaskCreate/TaskUpdate.
    todo_write: bool,
}

struct Step {
    id: String,
    subject: String,
    active: String,
    state: String,
}

impl TaskSteps {
    pub(super) fn snapshot(&self) -> Option<TaskProgress> {
        if self.incomplete || self.tasks.is_empty() {
            return None;
        }
        Some(TaskProgress {
            total: self.tasks.len() as u32,
            completed: self.tasks.iter().filter(|t| t.state == "completed").count() as u32,
            current_step: self
                .tasks
                .iter()
                .find(|t| t.state == "in_progress")
                .map(|t| t.active.clone())
                .or_else(|| {
                    self.tasks
                        .iter()
                        .find(|t| t.state == "pending")
                        .map(|t| t.subject.clone())
                }),
        })
    }

    // Only accepted run/session/turn Hooks reach here. No transcript reads or background polling.
    pub(super) fn observe(&mut self, event: &AgentHookEvent) {
        if event.profile != AgentProfile::Claude {
            return;
        }
        match event.provider_event.as_str() {
            "SessionStart" => {
                *self = Self::default();
                return;
            }
            "UserPromptSubmit" => {
                if self.turn != event.turn_id {
                    *self = Self {
                        turn: event.turn_id.clone(),
                        ..Self::default()
                    };
                }
                return;
            }
            "PostToolUse" if self.turn.is_some() && self.turn == event.turn_id => {}
            _ => return,
        }
        if self.incomplete {
            return;
        }
        let tool = event.tool_name.as_deref().unwrap_or_default();
        if !matches!(tool, "TaskCreate" | "TaskUpdate" | "TodoWrite") {
            return;
        }
        let raw = &event.raw_json;
        if raw["tool_response"]["success"] == false {
            return;
        }
        if self.apply(tool, raw).is_none() {
            // A gap invalidates this turn's totals; only a new observed turn can reset them.
            self.incomplete = true;
            self.tasks.clear();
            self.seen.clear();
        }
    }

    fn apply(&mut self, tool: &str, raw: &Value) -> Option<()> {
        if tool == "TodoWrite" {
            return self.apply_todo_write(raw);
        }
        // Subagent task lists are not the main turn's plan.
        if raw.get("agent_id").is_some() {
            return None;
        }
        let call = text(&raw["tool_use_id"])?;
        if self.seen.contains(call) {
            return Some(());
        }
        // ponytail: keep at most 64 steps and 256 confirmed changes per turn; beyond that hide
        // totals until a new turn. TodoWrite uses the same ceilings on one complete list.
        if self.seen.len() >= 256 || self.todo_write {
            return None;
        }
        let input = &raw["tool_input"];
        let response = &raw["tool_response"];
        match tool {
            "TaskCreate" => {
                let task = &response["task"];
                let id = text(&task["id"])?;
                let subject = text(&task["subject"])?;
                if Some(subject) != text(&input["subject"])
                    || self.tasks.len() >= 64
                    || self.tasks.iter().any(|t| t.id == id)
                {
                    return None;
                }
                let active = match input.get("activeForm") {
                    Some(value) => text(value)?,
                    None => subject,
                };
                self.tasks.push(Step {
                    id: id.into(),
                    subject: subject.into(),
                    active: active.into(),
                    state: "pending".into(),
                });
            }
            "TaskUpdate" => {
                if response["success"] != true {
                    return None;
                }
                let id = text(&response["taskId"])?;
                if Some(id) != text(&input["taskId"]) {
                    return None;
                }
                let step = self.tasks.iter_mut().find(|t| t.id == id)?;
                // Claude confirms an already-applied status with updatedFields=[] and no transition.
                if response.get("statusChange").is_none()
                    && response["updatedFields"]
                        .as_array()
                        .is_some_and(Vec::is_empty)
                    && text(&input["status"]) == Some(step.state.as_str())
                {
                    self.seen.insert(call.into());
                    return Some(());
                }
                let from = text(&response["statusChange"]["from"])?;
                let to = text(&response["statusChange"]["to"])?;
                if from != step.state
                    || Some(to) != text(&input["status"])
                    || !matches!(to, "pending" | "in_progress" | "completed")
                {
                    return None;
                }
                if let Some(subject) = input.get("subject") {
                    step.subject = text(subject)?.into();
                }
                if let Some(active) = input.get("activeForm") {
                    step.active = text(active)?.into();
                }
                step.state = to.into();
            }
            _ => return None,
        }
        self.seen.insert(call.into());
        Some(())
    }

    // Claude Code 2.1.296 replaces the whole list: tool_input.todos == tool_response.newTodos.
    // agent_id is present only for a subagent, so those lists are ignored.
    fn apply_todo_write(&mut self, raw: &Value) -> Option<()> {
        if raw.get("agent_id").is_some() {
            return Some(());
        }
        let call = text(&raw["tool_use_id"])?;
        if self.seen.contains(call) {
            return Some(());
        }
        if self.seen.len() >= 256 || (!self.todo_write && !self.tasks.is_empty()) {
            return None;
        }
        let steps = confirmed_todo_list(raw)?;
        if steps.len() > 64 {
            return None;
        }
        self.todo_write = true;
        self.tasks = steps;
        self.seen.insert(call.into());
        Some(())
    }
}

fn confirmed_todo_list(raw: &Value) -> Option<Vec<Step>> {
    let input = raw["tool_input"]["todos"].as_array()?;
    let confirmed = raw["tool_response"]["newTodos"].as_array()?;
    let previous = raw["tool_response"]["oldTodos"].as_array()?;
    if input.len() != confirmed.len() || previous.iter().any(|item| todo_step(item).is_none()) {
        return None;
    }
    let mut steps = Vec::with_capacity(input.len());
    for (index, (left, right)) in input.iter().zip(confirmed).enumerate() {
        let step = todo_step(left)?;
        let other = todo_step(right)?;
        if step.subject != other.subject || step.active != other.active || step.state != other.state
        {
            return None;
        }
        steps.push(Step {
            id: index.to_string(),
            subject: step.subject,
            active: step.active,
            state: step.state,
        });
    }
    Some(steps)
}

fn todo_step(value: &Value) -> Option<Step> {
    let state = text(&value["status"])?;
    if !matches!(state, "pending" | "in_progress" | "completed") {
        return None;
    }
    Some(Step {
        id: String::new(),
        subject: text(&value["content"])?.into(),
        active: text(&value["activeForm"])?.into(),
        state: state.into(),
    })
}

fn text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.chars().count() <= 256)
}
