use crate::{
    current_unix_timestamp, migrations::open_project_connection, StorageError, StorageResult,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentDispatchTaskState {
    Submitted,
    Queued,
    Untracked,
    Done,
    Failed,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentDispatchTracking {
    Managed,
    Untracked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAgentDispatchTask {
    pub definition_id: i64,
    pub node_id: u64,
    pub original_instruction: String,
    pub task_text: String,
    pub state: AgentDispatchTaskState,
    pub tracking: AgentDispatchTracking,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredAgentDispatchTask {
    pub id: i64,
    pub definition_id: i64,
    pub node_id: u64,
    pub original_instruction: String,
    pub task_text: String,
    pub state: AgentDispatchTaskState,
    pub tracking: AgentDispatchTracking,
    pub error: Option<String>,
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
             (definition_id, node_id, original_instruction, task_text, state, tracking,
              created_at_unix, updated_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
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

    pub fn list(&self) -> StorageResult<Vec<StoredAgentDispatchTask>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT id, definition_id, node_id, original_instruction, task_text, state,
                    tracking, error
             FROM agent_dispatch_tasks ORDER BY id",
        )?;
        let rows = statement.query_map([], decode_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn finish_current(
        &self,
        definition_id: i64,
        state: AgentDispatchTaskState,
        error: Option<&str>,
    ) -> StorageResult<Option<i64>> {
        let mut connection = open_project_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = transaction
            .query_row(
                "SELECT id FROM agent_dispatch_tasks
                 WHERE definition_id = ?1 AND state = 'submitted'
                 ORDER BY id LIMIT 1",
                [definition_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(id) = id {
            transaction.execute(
                "UPDATE agent_dispatch_tasks
                 SET state = ?1, error = ?2, updated_at_unix = ?3 WHERE id = ?4",
                params![encode_state(state), error, current_unix_timestamp()?, id],
            )?;
        }
        transaction.commit()?;
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

    pub fn mark_incomplete_interrupted(&self) -> StorageResult<Vec<i64>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT id FROM agent_dispatch_tasks
             WHERE state IN ('submitted', 'queued', 'untracked') ORDER BY id",
        )?;
        let ids = statement
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        if !ids.is_empty() {
            connection.execute(
                "UPDATE agent_dispatch_tasks
                 SET state = 'interrupted', updated_at_unix = ?1
                 WHERE state IN ('submitted', 'queued', 'untracked')",
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
             WHERE definition_id = ?2 AND state IN ('submitted', 'queued', 'untracked')",
            params![current_unix_timestamp()?, definition_id],
        )?)
    }

    fn get(&self, id: i64) -> StorageResult<Option<StoredAgentDispatchTask>> {
        let connection = open_project_connection(&self.path)?;
        connection
            .query_row(
                "SELECT id, definition_id, node_id, original_instruction, task_text, state,
                        tracking, error
                 FROM agent_dispatch_tasks WHERE id = ?1",
                [id],
                decode_row,
            )
            .optional()
            .map_err(StorageError::from)
    }
}

fn decode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredAgentDispatchTask> {
    let state: String = row.get(5)?;
    let tracking: String = row.get(6)?;
    Ok(StoredAgentDispatchTask {
        id: row.get(0)?,
        definition_id: row.get(1)?,
        node_id: row.get(2)?,
        original_instruction: row.get(3)?,
        task_text: row.get(4)?,
        state: decode_state(&state)?,
        tracking: decode_tracking(&tracking)?,
        error: row.get(7)?,
    })
}

fn encode_state(state: AgentDispatchTaskState) -> &'static str {
    match state {
        AgentDispatchTaskState::Submitted => "submitted",
        AgentDispatchTaskState::Queued => "queued",
        AgentDispatchTaskState::Untracked => "untracked",
        AgentDispatchTaskState::Done => "done",
        AgentDispatchTaskState::Failed => "failed",
        AgentDispatchTaskState::Interrupted => "interrupted",
    }
}

fn decode_state(value: &str) -> rusqlite::Result<AgentDispatchTaskState> {
    match value {
        "submitted" => Ok(AgentDispatchTaskState::Submitted),
        "queued" => Ok(AgentDispatchTaskState::Queued),
        "untracked" => Ok(AgentDispatchTaskState::Untracked),
        "done" => Ok(AgentDispatchTaskState::Done),
        "failed" => Ok(AgentDispatchTaskState::Failed),
        "interrupted" => Ok(AgentDispatchTaskState::Interrupted),
        other => Err(invalid_text(other)),
    }
}

fn encode_tracking(tracking: AgentDispatchTracking) -> &'static str {
    match tracking {
        AgentDispatchTracking::Managed => "managed",
        AgentDispatchTracking::Untracked => "untracked",
    }
}

fn decode_tracking(value: &str) -> rusqlite::Result<AgentDispatchTracking> {
    match value {
        "managed" => Ok(AgentDispatchTracking::Managed),
        "untracked" => Ok(AgentDispatchTracking::Untracked),
        other => Err(invalid_text(other)),
    }
}

fn invalid_text(value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        value.len(),
        rusqlite::types::Type::Text,
        Box::new(StorageError::InvalidAgentDispatchValue(value.to_string())),
    )
}

#[cfg(test)]
#[path = "../tests/unit/agent_dispatch_store_tests.rs"]
mod tests;
