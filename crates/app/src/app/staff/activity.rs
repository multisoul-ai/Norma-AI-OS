//! Transient task progress. Durable Task.messages remains the conversation authority.
use brain::acp::trace::TraceKind;
use brain::event::TaskUpdate;
use norma::protocol::{Task, TaskMessage};

#[derive(Default)]
pub(crate) struct TaskActivity {
    pub turn: u64,
    pub queued: usize,
    pub active: bool,
    pub reply: String,
    pub plan: Vec<(String, String)>,
    pub failed: bool,
    pub working: bool,
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
                TraceKind::ToolCall => self.working = true,
                TraceKind::ToolResult => self.working = false,
                _ => {}
            },
            TaskUpdate::Finished { failed } if turn == self.turn => {
                self.active = false;
                self.working = false;
                self.failed = failed;
                // Successful completion is sent only after durable TaskChanged.
                if !failed {
                    self.reply.clear();
                }
            }
            _ => {}
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
        self.working = false;
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/staff_activity_tests.rs"]
mod tests;
