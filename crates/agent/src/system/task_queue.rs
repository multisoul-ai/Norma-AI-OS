use super::{AgentSystem, PendingTask};
use crate::journal::{AuditEntry, CardTaskJournal};
use crate::{AgentEvent, AgentProvider, CardToolHost, TaskId, TaskRecord, TaskState};
use norma::protocol::AgentTaskAssignment;

impl<D, C, H, J> AgentSystem<D, C, H, J>
where
    D: AgentProvider,
    C: AgentProvider,
    H: CardToolHost,
    J: CardTaskJournal,
{
    pub(super) fn enqueue_task(
        &mut self,
        trace_id: String,
        intent: String,
        delegation_assignments: Option<Vec<AgentTaskAssignment>>,
    ) -> TaskId {
        let task_id = self.next_task_id;
        self.next_task_id += 1;
        let record = TaskRecord {
            project_id: self.project_id.clone(),
            trace_id: trace_id.clone(),
            task_id,
            intent: intent.clone(),
            state: TaskState::Queued,
            summary: None,
            error: None,
            completed_tool_calls: 0,
        };
        self.journal.append(AuditEntry::Task(record.clone()));
        self.records.insert(task_id, record);
        self.pending.push_back(PendingTask {
            task_id,
            trace_id: trace_id.clone(),
            delegation_assignments,
        });
        tracing::info!(
            project_id = %self.project_id,
            trace_id,
            task_id,
            intent_chars = intent.chars().count(),
            "card task queued by dialogue"
        );
        self.emit_event(AgentEvent::TaskQueued {
            task_id,
            summary: intent,
        });
        task_id
    }
}
