use crate::{TaskRecord, ToolCallRecord, ToolErrorRecord};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuditEntry {
    Task(TaskRecord),
    ToolCall(ToolCallRecord),
    ToolError(ToolErrorRecord),
}

pub trait CardTaskJournal: Send {
    fn append(&mut self, entry: AuditEntry);

    fn next_task_id(&self, _project_id: &str) -> u64 {
        1
    }

    fn recover_incomplete(&mut self, _project_id: &str) -> Vec<u64> {
        Vec::new()
    }
}

#[derive(Clone, Debug, Default)]
pub struct MemoryJournal {
    entries: Vec<AuditEntry>,
}

impl MemoryJournal {
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }
}

impl CardTaskJournal for MemoryJournal {
    fn append(&mut self, entry: AuditEntry) {
        self.entries.push(entry);
    }

    fn next_task_id(&self, project_id: &str) -> u64 {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                AuditEntry::Task(task) if task.project_id == project_id => Some(task.task_id),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            + 1
    }
}

impl AuditEntry {
    pub fn task_id(&self) -> u64 {
        match self {
            Self::Task(record) => record.task_id,
            Self::ToolCall(record) => record.task_id,
            Self::ToolError(record) => record.task_id,
        }
    }

    pub fn tool_call_id(&self) -> Option<&str> {
        match self {
            Self::Task(_) => None,
            Self::ToolCall(record) => Some(record.tool_call_id.as_str()),
            Self::ToolError(record) => Some(record.tool_call_id.as_str()),
        }
    }
}
