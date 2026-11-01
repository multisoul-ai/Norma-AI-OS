use crate::{
    current_unix_timestamp, migrations::open_project_connection, StorageError, StorageResult,
};
use codec::{decode_row, encode_state, encode_tracking};

mod codec;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentDispatchTaskState {
    Accepted,
    Submitted,
    Queued,
    Untracked,
    Done,
    Failed,
    CancelRequested,
    Cancelled,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentDispatchTracking {
    Managed,
    Untracked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAgentDispatchTask {
    pub runtime_task_id: u64,
    pub definition_id: i64,
    pub node_id: u64,
    pub original_instruction: String,
    pub task_text: String,
    pub state: AgentDispatchTaskState,
    pub tracking: AgentDispatchTracking,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoredAgentDispatchResult {
    pub text: Option<String>,
    pub truncated: bool,
    #[serde(default)]
    pub background_unsettled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredAgentDispatchTask {
    pub id: i64,
    pub runtime_instance_id: String,
    pub runtime_task_id: u64,
    pub task_ref: String,
    pub root_task_ref: String,
    pub parent_task_ref: Option<String>,
    pub source_definition_id: Option<i64>,
    pub source_node_id: Option<u64>,
    pub source_provider: Option<String>,
    pub definition_id: i64,
    pub node_id: u64,
    pub original_instruction: String,
    pub task_text: String,
    pub state: AgentDispatchTaskState,
    pub tracking: AgentDispatchTracking,
    pub error: Option<String>,
    pub result: Option<StoredAgentDispatchResult>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewOrchestrationAgentDispatchTask {
    pub runtime_instance_id: String,
    pub runtime_task_id: u64,
    pub task_ref: String,
    pub root_task_ref: String,
    pub parent_task_ref: Option<String>,
    pub source_definition_id: Option<i64>,
    pub source_node_id: Option<u64>,
    pub source_provider: Option<String>,
    pub target_definition_id: i64,
    pub target_node_id: u64,
    pub original_instruction: String,
    pub task_text: String,
    pub state: AgentDispatchTaskState,
    pub tracking: AgentDispatchTracking,
}

#[derive(Clone, Debug)]
pub struct AgentDispatchTaskStore {
    path: PathBuf,
}

impl AgentDispatchTaskStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn create(&self, task: &NewAgentDispatchTask) -> StorageResult<StoredAgentDispatchTask> {
        let connection = open_project_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        connection.execute(
            "INSERT INTO agent_dispatch_tasks
             (runtime_instance_id, runtime_task_id, task_ref, root_task_ref,
              definition_id, node_id, original_instruction, task_text, state, tracking,
              created_at_unix, updated_at_unix)
             VALUES ('', ?1, '', '', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                task.runtime_task_id,
                task.definition_id,
                task.node_id,
                task.original_instruction,
                task.task_text,
                encode_state(task.state),
                encode_tracking(task.tracking),
                now,
            ],
        )?;
        let id = connection.last_insert_rowid();
        self.get(id)?
            .ok_or(StorageError::AgentDispatchTaskNotFound(id))
    }

    pub fn create_orchestration(
        &self,
        task: &NewOrchestrationAgentDispatchTask,
    ) -> StorageResult<StoredAgentDispatchTask> {
        let connection = open_project_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        connection.execute(
            "INSERT INTO agent_dispatch_tasks
             (runtime_instance_id, runtime_task_id, task_ref, root_task_ref, parent_task_ref,
              source_definition_id, source_node_id, source_provider, definition_id, node_id,
              original_instruction, task_text, state, tracking, created_at_unix, updated_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
            params![
                task.runtime_instance_id,
                task.runtime_task_id,
                task.task_ref,
                task.root_task_ref,
                task.parent_task_ref,
                task.source_definition_id,
                task.source_node_id,
                task.source_provider,
                task.target_definition_id,
                task.target_node_id,
                task.original_instruction,
                task.task_text,
                encode_state(task.state),
                encode_tracking(task.tracking),
                now,
            ],
        )?;
        let id = connection.last_insert_rowid();
        self.get(id)?
            .ok_or(StorageError::AgentDispatchTaskNotFound(id))
    }

    pub fn list(&self) -> StorageResult<Vec<StoredAgentDispatchTask>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT id, runtime_instance_id, runtime_task_id, task_ref, root_task_ref,
                    parent_task_ref, source_definition_id, source_node_id, source_provider,
                    definition_id, node_id, original_instruction, task_text, state, tracking, error,
                    result_captured, result_text, result_truncated,
                    result_background_unsettled
             FROM agent_dispatch_tasks ORDER BY id",
        )?;
        let rows = statement.query_map([], decode_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_by_task_ref(
        &self,
        task_ref: &str,
    ) -> StorageResult<Option<StoredAgentDispatchTask>> {
        // A recovery lookup must not create or migrate a missing/replaced project database.
        if !self.path.try_exists()? {
            return Ok(None);
        }
        let connection = rusqlite::Connection::open_with_flags(
            &self.path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        connection
            .query_row(
                "SELECT id, runtime_instance_id, runtime_task_id, task_ref, root_task_ref,
                        parent_task_ref, source_definition_id, source_node_id, source_provider,
                        definition_id, node_id, original_instruction, task_text, state, tracking, error,
                        result_captured, result_text, result_truncated,
                        result_background_unsettled
                 FROM agent_dispatch_tasks WHERE task_ref = ?1 ORDER BY id DESC LIMIT 1",
                [task_ref],
                decode_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn finish_current(
        &self,
        definition_id: i64,
        state: AgentDispatchTaskState,
        error: Option<&str>,
    ) -> StorageResult<Option<i64>> {
        self.finish_current_with_result(definition_id, state, error, None)
    }

    pub fn finish_current_with_result(
        &self,
        definition_id: i64,
        state: AgentDispatchTaskState,
        error: Option<&str>,
        result: Option<&StoredAgentDispatchResult>,
    ) -> StorageResult<Option<i64>> {
        let mut connection = open_project_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = transaction
            .query_row(
                "SELECT id FROM agent_dispatch_tasks
                 WHERE definition_id = ?1 AND state IN ('submitted', 'untracked', 'cancel_requested')
                 ORDER BY id LIMIT 1",
                [definition_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(id) = id {
            transaction.execute(
                "UPDATE agent_dispatch_tasks
                 SET state = ?1, error = ?2, result_captured = ?3, result_text = ?4,
                     result_truncated = ?5, result_background_unsettled = ?6,
                     updated_at_unix = ?7 WHERE id = ?8",
                params![
                    encode_state(state),
                    error,
                    result.is_some() as i64,
                    result.and_then(|value| value.text.as_deref()),
                    result.is_some_and(|value| value.truncated) as i64,
                    result.is_some_and(|value| value.background_unsettled) as i64,
                    current_unix_timestamp()?,
                    id
                ],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn finish_task(
        &self,
        runtime_task_id: u64,
        state: AgentDispatchTaskState,
        error: Option<&str>,
    ) -> StorageResult<Option<i64>> {
        self.finish_task_with_result(runtime_task_id, state, error, None)
    }

    pub fn finish_task_with_result(
        &self,
        runtime_task_id: u64,
        state: AgentDispatchTaskState,
        error: Option<&str>,
        result: Option<&StoredAgentDispatchResult>,
    ) -> StorageResult<Option<i64>> {
        let connection = open_project_connection(&self.path)?;
        let id = connection
            .query_row(
                "SELECT id FROM agent_dispatch_tasks
                 WHERE runtime_task_id = ?1
                   AND state IN ('accepted', 'queued', 'submitted', 'untracked', 'cancel_requested')
                 ORDER BY id DESC LIMIT 1",
                [runtime_task_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(id) = id {
            connection.execute(
                "UPDATE agent_dispatch_tasks
                 SET state = ?1, error = ?2, result_captured = ?3, result_text = ?4,
                     result_truncated = ?5, result_background_unsettled = ?6,
                     updated_at_unix = ?7 WHERE id = ?8",
                params![
                    encode_state(state),
                    error,
                    result.is_some() as i64,
                    result.and_then(|value| value.text.as_deref()),
                    result.is_some_and(|value| value.truncated) as i64,
                    result.is_some_and(|value| value.background_unsettled) as i64,
                    current_unix_timestamp()?,
                    id
                ],
            )?;
        }
        Ok(id)
    }

    pub fn persist_cancellation(
        &self,
        runtime_task_id: u64,
        running: bool,
    ) -> StorageResult<Option<i64>> {
        let connection = open_project_connection(&self.path)?;
        let (source_states, target_state) = if running {
            ("'submitted', 'untracked'", "cancel_requested")
        } else {
            ("'accepted', 'queued'", "cancelled")
        };
        let id = connection
            .query_row(
                &format!(
                    "SELECT id FROM agent_dispatch_tasks
                     WHERE runtime_task_id = ?1 AND state IN ({source_states})
                     ORDER BY id DESC LIMIT 1"
                ),
                [runtime_task_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(id) = id {
            connection.execute(
                "UPDATE agent_dispatch_tasks
                 SET state = ?1, updated_at_unix = ?2 WHERE id = ?3",
                params![target_state, current_unix_timestamp()?, id],
            )?;
        }
        Ok(id)
    }

    pub fn submit_next_queued(&self, definition_id: i64) -> StorageResult<Option<i64>> {
        let mut connection = open_project_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = transaction
            .query_row(
                "SELECT id FROM agent_dispatch_tasks
                 WHERE definition_id = ?1 AND state = 'queued'
                 ORDER BY id LIMIT 1",
                [definition_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(id) = id {
            transaction.execute(
                "UPDATE agent_dispatch_tasks
                 SET state = 'submitted', updated_at_unix = ?1 WHERE id = ?2",
                params![current_unix_timestamp()?, id],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn fail_next_queued(&self, definition_id: i64, error: &str) -> StorageResult<Option<i64>> {
        let mut connection = open_project_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = transaction
            .query_row(
                "SELECT id FROM agent_dispatch_tasks
                 WHERE definition_id = ?1 AND state = 'queued'
                 ORDER BY id LIMIT 1",
                [definition_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(id) = id {
            transaction.execute(
                "UPDATE agent_dispatch_tasks
                 SET state = 'failed', error = ?1, updated_at_unix = ?2 WHERE id = ?3",
                params![error, current_unix_timestamp()?, id],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn mark_incomplete_interrupted(&self) -> StorageResult<Vec<i64>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT id FROM agent_dispatch_tasks
             WHERE state IN ('accepted', 'submitted', 'queued', 'untracked', 'cancel_requested')
             ORDER BY id",
        )?;
        let ids = statement
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        if !ids.is_empty() {
            connection.execute(
                "UPDATE agent_dispatch_tasks
                 SET state = 'interrupted', updated_at_unix = ?1
                 WHERE state IN ('accepted', 'submitted', 'queued', 'untracked', 'cancel_requested')",
                [current_unix_timestamp()?],
            )?;
        }
        Ok(ids)
    }

    pub fn interrupt_definition(&self, definition_id: i64) -> StorageResult<usize> {
        let connection = open_project_connection(&self.path)?;
        Ok(connection.execute(
            "UPDATE agent_dispatch_tasks
             SET state = 'interrupted', updated_at_unix = ?1
             WHERE definition_id = ?2
               AND state IN ('accepted', 'submitted', 'queued', 'untracked', 'cancel_requested')",
            params![current_unix_timestamp()?, definition_id],
        )?)
    }

    fn get(&self, id: i64) -> StorageResult<Option<StoredAgentDispatchTask>> {
        let connection = open_project_connection(&self.path)?;
        connection
            .query_row(
                "SELECT id, runtime_instance_id, runtime_task_id, task_ref, root_task_ref,
                        parent_task_ref, source_definition_id, source_node_id, source_provider,
                        definition_id, node_id, original_instruction, task_text, state, tracking, error,
                        result_captured, result_text, result_truncated,
                        result_background_unsettled
                 FROM agent_dispatch_tasks WHERE id = ?1",
                [id],
                decode_row,
            )
            .optional()
            .map_err(StorageError::from)
    }
}

#[cfg(test)]
#[path = "../tests/unit/agent_dispatch_store_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/orchestration_dispatch_audit_tests.rs"]
mod orchestration_tests;
