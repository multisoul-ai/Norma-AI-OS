use super::operation::new_operation_id;
use super::{AgentTaskArgs, AgentTaskCommand};
use crate::protocol::{AgentTaskAssignment, CardCommand};

impl AgentTaskArgs {
    pub(super) fn into_command(self) -> (CardCommand, Option<u64>, Option<String>, Option<String>) {
        match self.command {
            AgentTaskCommand::Assign {
                agent,
                workspace,
                task,
                operation_id,
                force_untracked,
            } => (
                CardCommand::DispatchAgentTasks {
                    assignments: vec![AgentTaskAssignment { name: agent, task }],
                    force_untracked,
                },
                None,
                workspace,
                Some(operation_id.unwrap_or_else(new_operation_id)),
            ),
            AgentTaskCommand::Status { task_id } => (
                CardCommand::AgentTaskStatus { task_ref: task_id },
                None,
                None,
                None,
            ),
            AgentTaskCommand::Result { task_id } => (
                CardCommand::AgentTaskResult { task_ref: task_id },
                None,
                None,
                None,
            ),
            AgentTaskCommand::Wait { task_id, timeout } => (
                CardCommand::AgentTaskStatus { task_ref: task_id },
                Some(timeout),
                None,
                None,
            ),
            AgentTaskCommand::Cancel {
                task_id,
                operation_id,
            } => (
                CardCommand::AgentTaskCancel { task_ref: task_id },
                None,
                None,
                Some(operation_id.unwrap_or_else(new_operation_id)),
            ),
        }
    }
}
