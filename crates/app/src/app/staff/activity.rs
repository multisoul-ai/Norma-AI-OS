//! Transient task progress. Durable Task.messages remains the conversation authority.
use brain::acp::trace::{TraceItem, TraceKind};
use brain::event::TaskUpdate;
use norma::protocol::{Task, TaskMessage};
use std::time::Instant;

#[derive(Default)]
pub(crate) struct TaskActivity {
    pub turn: u64,
    pub queued: usize,
    pub active: bool,
    pub reply: String,
    pub plan: Vec<(String, String)>,
    pub failed: bool,
    pub tools: Vec<ToolActivity>,
    started: Option<Instant>,
    pub elapsed_ms: Option<u64>,
    baseline: usize,
    committed: bool,
}

impl TaskActivity {
    pub fn waiting(&self) -> bool {
        self.active || self.queued > 0
    }

    pub fn apply(&mut self, turn: u64, update: TaskUpdate) {
        match update {
            TaskUpdate::Queued => self.queued += 1,
            TaskUpdate::Started { message_count } if turn > self.turn => {
                *self = Self {
                    turn,
                    queued: self.queued.saturating_sub(1),
                    active: true,
                    started: Some(Instant::now()),
                    baseline: message_count,
                    ..Self::default()
                };
            }
            TaskUpdate::Update(item) if turn == self.turn && self.active => match item.kind {
                TraceKind::Reply if !self.committed => self.reply.push_str(&item.text),
                TraceKind::Plan => {
                    self.plan = item
                        .text
                        .lines()
                        .filter_map(|line| {
                            let (status, text) = line.strip_prefix('[')?.split_once("] ")?;
                            Some((status.to_owned(), text.to_owned()))
                        })
                        .collect()
                }
                TraceKind::ToolCall | TraceKind::ToolResult | TraceKind::Permission => {
                    self.update_tool(item)
                }
                _ => {}
            },
            TaskUpdate::Finished { failed } if turn == self.turn => {
                self.active = false;
                self.finish_tools();
                self.failed = failed;
                // Successful completion is sent only after durable TaskChanged.
                if !failed {
                    self.reply.clear();
                }
            }
            _ => {}
        }
    }

    pub fn working(&self) -> bool {
        self.tools
            .iter()
            .any(|tool| tool.status == ToolStatus::Running)
    }

    pub fn duration_ms(&self) -> u64 {
        self.elapsed_ms.unwrap_or_else(|| {
            self.started
                .map_or(0, |start| start.elapsed().as_millis() as u64)
        })
    }

    fn update_tool(&mut self, item: TraceItem) {
        let Some(meta) = item.tool else { return };
        let index = self
            .tools
            .iter()
            .position(|tool| tool.id == meta.id)
            .unwrap_or_else(|| {
                self.tools.push(ToolActivity {
                    id: meta.id.clone(),
                    name: meta.id.clone(),
                    input: String::new(),
                    output: String::new(),
                    status: ToolStatus::Running,
                    started: Instant::now(),
                    elapsed_ms: None,
                });
                self.tools.len() - 1
            });
        let tool = &mut self.tools[index];
        if let Some(name) = meta.name {
            tool.name = name;
        }
        if let Some(input) = meta.input {
            tool.input = input;
        }
        if !item.text.is_empty()
            && matches!(item.kind, TraceKind::ToolResult | TraceKind::Permission)
        {
            tool.output = item.text;
        }
        if tool.status == ToolStatus::Denied {
            return;
        }
        let status = match meta.status.as_deref() {
            Some("denied") => ToolStatus::Denied,
            Some("completed") => ToolStatus::Completed,
            Some("failed") => ToolStatus::Failed,
            _ => tool.status,
        };
        tool.status = status;
        if status != ToolStatus::Running && tool.elapsed_ms.is_none() {
            tool.elapsed_ms = Some(tool.started.elapsed().as_millis() as u64);
        }
    }

    fn finish_tools(&mut self) {
        self.elapsed_ms = Some(self.duration_ms());
        for tool in &mut self.tools {
            if tool.status == ToolStatus::Running {
                tool.status = ToolStatus::Interrupted;
                tool.elapsed_ms = Some(tool.started.elapsed().as_millis() as u64);
            }
        }
    }

    pub fn reconcile(&mut self, task: &Task) {
        if task
            .messages
            .iter()
            .skip(self.baseline)
            .any(|m| matches!(m, TaskMessage::Norma { .. }))
        {
            self.reply.clear();
            self.committed = true;
        }
    }

    pub fn interrupt(&mut self) {
        if self.waiting() {
            self.failed = true;
        }
        self.active = false;
        self.queued = 0;
        self.finish_tools();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolStatus {
    Running,
    Completed,
    Failed,
    Denied,
    Interrupted,
}

pub(crate) struct ToolActivity {
    pub id: String,
    pub name: String,
    pub input: String,
    pub output: String,
    pub status: ToolStatus,
    started: Instant,
    pub elapsed_ms: Option<u64>,
}

#[cfg(test)]
#[path = "../../../tests/unit/staff_activity_tests.rs"]
mod tests;
