use crate::{
    current_unix_timestamp, migrations::open_project_connection, StorageError, StorageResult,
};
use agent::{AuditEntry, CardTaskJournal, TaskRecord, TaskState, ToolCallRecord, ToolErrorRecord};
use rusqlite::params;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredAgentTask {
    pub task: TaskRecord,
    pub tool_calls: Vec<ToolCallRecord>,
    pub tool_errors: Vec<ToolErrorRecord>,
}

#[derive(Clone, Debug)]
pub struct AgentTaskStore {
    path: PathBuf,
}

impl AgentTaskStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load_project(&self, project_id: &str) -> StorageResult<Vec<StoredAgentTask>> {
        let connection = open_project_connection(&self.path)?;
        let mut tasks = Vec::new();
        let mut statement = connection.prepare(
            "SELECT task_id, trace_id, intent, state, summary, error, completed_tool_calls
             FROM agent_tasks WHERE project_id = ?1 ORDER BY task_id",
        )?;
        let rows = statement.query_map([project_id], |row| {
            Ok(TaskRecord {
                project_id: project_id.to_string(),
                trace_id: row.get(1)?,
                task_id: row.get(0)?,
                intent: row.get(2)?,
                state: parse_task_state(&row.get::<_, String>(3)?, row.get(5)?)?,
                summary: row.get(4)?,
                error: row.get(5)?,
                completed_tool_calls: row.get::<_, i64>(6)? as usize,
            })
        })?;
        for row in rows {
            let task = row?;
            let tool_calls = self.load_tool_calls(&connection, project_id, task.task_id)?;
            let tool_errors = self.load_tool_errors(&connection, project_id, task.task_id)?;
            tasks.push(StoredAgentTask {
                task,
                tool_calls,
                tool_errors,
            });
        }
        Ok(tasks)
    }

    pub fn mark_incomplete_interrupted(&self, project_id: &str) -> StorageResult<Vec<u64>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT task_id FROM agent_tasks
             WHERE project_id = ?1 AND state IN ('queued', 'running')
             ORDER BY task_id",
        )?;
        let ids = statement
            .query_map([project_id], |row| row.get::<_, u64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for task_id in &ids {
            connection.execute(
                "UPDATE agent_tasks SET state = 'interrupted', updated_at_unix = ?1
                 WHERE project_id = ?2 AND task_id = ?3",
                params![current_unix_timestamp()?, project_id, task_id],
            )?;
        }
        Ok(ids)
    }

    fn load_tool_calls(
        &self,
        connection: &rusqlite::Connection,
        project_id: &str,
        task_id: u64,
    ) -> StorageResult<Vec<ToolCallRecord>> {
        let mut statement = connection.prepare(
            "SELECT tool_call_id, command_json, result_json FROM agent_tool_calls
             WHERE project_id = ?1 AND task_id = ?2 ORDER BY created_at_unix, tool_call_id",
        )?;
        let rows = statement.query_map(params![project_id, task_id], |row| {
            let command_json: String = row.get(1)?;
            let result_json: String = row.get(2)?;
            Ok(ToolCallRecord {
                project_id: project_id.to_string(),
                task_id,
                tool_call_id: row.get(0)?,
                command: serde_json::from_str(&command_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        command_json.len(),
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                result: serde_json::from_str(&result_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        result_json.len(),
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn load_tool_errors(
        &self,
        connection: &rusqlite::Connection,
        project_id: &str,
        task_id: u64,
    ) -> StorageResult<Vec<ToolErrorRecord>> {
        let mut statement = connection.prepare(
            "SELECT tool_call_id, tool_name, arguments_json, error FROM agent_tool_errors
             WHERE project_id = ?1 AND task_id = ?2 ORDER BY created_at_unix, tool_call_id",
        )?;
        let rows = statement.query_map(params![project_id, task_id], |row| {
            Ok(ToolErrorRecord {
                project_id: project_id.to_string(),
                task_id,
                tool_call_id: row.get(0)?,
                tool_name: row.get(1)?,
                arguments_json: row.get(2)?,
                error: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

impl CardTaskJournal for AgentTaskStore {
    fn append(&mut self, entry: AuditEntry) {
        if let Err(error) = self.append_inner(entry) {
            tracing::error!(%error, "failed to persist agent task audit entry");
        }
    }

    fn recover_incomplete(&mut self, project_id: &str) -> Vec<u64> {
        self.mark_incomplete_interrupted(project_id)
            .unwrap_or_default()
    }

    fn next_task_id(&self, project_id: &str) -> u64 {
        open_project_connection(&self.path)
            .and_then(|connection| {
                connection.query_row(
                    "SELECT COALESCE(MAX(task_id), 0) + 1 FROM agent_tasks WHERE project_id = ?1",
                    [project_id],
                    |row| row.get(0),
                )
                .map_err(StorageError::from)
            })
            .unwrap_or(1)
    }
}

impl AgentTaskStore {
    pub fn append_audit(&self, entry: AuditEntry) -> StorageResult<()> {
        self.append_inner(entry)
    }

    fn append_inner(&self, entry: AuditEntry) -> StorageResult<()> {
        let connection = open_project_connection(&self.path)?;
        match entry {
            AuditEntry::Task(task) => {
                connection.execute(
                    "INSERT INTO agent_tasks
                        (project_id, trace_id, task_id, intent, state, summary, error,
                         completed_tool_calls, updated_at_unix)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     ON CONFLICT(project_id, task_id) DO UPDATE SET
                        trace_id = excluded.trace_id, intent = excluded.intent,
                        state = excluded.state,
                        summary = excluded.summary, error = excluded.error,
                        completed_tool_calls = excluded.completed_tool_calls,
                        updated_at_unix = excluded.updated_at_unix",
                    params![
                        task.project_id,
                        task.trace_id,
                        task.task_id,
                        task.intent,
                        task.state.storage_name(),
                        task.summary,
                        task.error,
                        task.completed_tool_calls as i64,
                        current_unix_timestamp()?,
                    ],
                )?;
            }
            AuditEntry::ToolCall(tool) => {
                let command_json = serde_json::to_string(&tool.command)?;
                let result_json = serde_json::to_string(&tool.result)?;
                connection.execute(
                    "INSERT OR IGNORE INTO agent_tool_calls
                        (project_id, task_id, tool_call_id, command_json, result_json, created_at_unix)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        tool.project_id,
                        tool.task_id,
                        tool.tool_call_id,
                        command_json,
                        result_json,
                        current_unix_timestamp()?,
                    ],
                )?;
            }
            AuditEntry::ToolError(tool) => {
                connection.execute(
                    "INSERT OR IGNORE INTO agent_tool_errors
                        (project_id, task_id, tool_call_id, tool_name, arguments_json, error, created_at_unix)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        tool.project_id,
                        tool.task_id,
                        tool.tool_call_id,
                        tool.tool_name,
                        tool.arguments_json,
                        tool.error,
                        current_unix_timestamp()?,
                    ],
                )?;
            }
        }
        Ok(())
    }
}

fn parse_task_state(state: &str, error: Option<String>) -> rusqlite::Result<TaskState> {
    match state {
        "queued" => Ok(TaskState::Queued),
        "running" => Ok(TaskState::Running),
        "succeeded" => Ok(TaskState::Succeeded),
        "partially_dispatched" => Ok(TaskState::PartiallyDispatched),
        "failed" => Ok(TaskState::Failed {
            reason: error.unwrap_or_else(|| "unknown failure".to_string()),
        }),
        "cancelled_with_partial_effects" => Ok(TaskState::CancelledWithPartialEffects),
        "cancelled" => Ok(TaskState::Cancelled),
        "interrupted" => Ok(TaskState::Interrupted),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            other.len(),
            rusqlite::types::Type::Text,
            Box::new(StorageError::InvalidWorkspaceSource(other.to_string())),
        )),
    }
}

#[cfg(test)]
#[path = "../tests/unit/agent_task_store_tests.rs"]
mod tests;
