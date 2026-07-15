use crate::{error::StorageError, StorageResult};
use rusqlite::Connection;
use std::path::Path;

pub(crate) fn open_app_connection(path: &Path) -> StorageResult<Connection> {
    let connection = open_connection(path)?;
    migrate_app_db(&connection)?;
    Ok(connection)
}

pub(crate) fn open_project_connection(path: &Path) -> StorageResult<Connection> {
    let connection = open_connection(path)?;
    migrate_project_db(&connection)?;
    Ok(connection)
}

fn open_connection(path: &Path) -> StorageResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(connection)
}

pub fn migrate_app_db(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS recent_workspaces (
            path TEXT PRIMARY KEY NOT NULL,
            display_name TEXT NOT NULL,
            last_opened_at_unix INTEGER NOT NULL,
            source TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL,
            updated_at_unix INTEGER NOT NULL
        );
        "#,
    )?;
    connection.pragma_update(None, "user_version", 1)?;
    Ok(())
}

pub fn migrate_project_db(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS canvas_snapshots (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            json TEXT NOT NULL,
            updated_at_unix INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS session_checkpoints (
            node_id INTEGER PRIMARY KEY NOT NULL,
            kind TEXT NOT NULL,
            payload TEXT NOT NULL,
            cols INTEGER NOT NULL DEFAULT 80,
            rows INTEGER NOT NULL DEFAULT 24,
            shell TEXT,
            agent_profile TEXT,
            updated_at_unix INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS node_sessions (
            node_id         INTEGER PRIMARY KEY NOT NULL,
            kind            TEXT    NOT NULL,
            cwd             TEXT,
            session_id      TEXT,
            updated_at_unix INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS layout_state (
            id                   INTEGER PRIMARY KEY CHECK (id = 1),
            mode                 TEXT NOT NULL,
            master_id            INTEGER,
            original_bounds_json TEXT NOT NULL,
            updated_at_unix      INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS voice_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            transcript TEXT NOT NULL,
            planner_json TEXT NOT NULL,
            confidence REAL NOT NULL,
            risk TEXT NOT NULL,
            status TEXT NOT NULL,
            provider TEXT,
            model TEXT,
            target_node_id INTEGER,
            created_at_unix INTEGER NOT NULL,
            error TEXT,
            raw_transcript TEXT NOT NULL DEFAULT '',
            refined_transcript TEXT
        );

        CREATE TABLE IF NOT EXISTS agent_definitions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            name_key TEXT,
            speech_alias TEXT,
            runtime_profile TEXT NOT NULL,
            workflow_description TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_definitions_name_nocase
        ON agent_definitions(name COLLATE NOCASE);

        CREATE TABLE IF NOT EXISTS agent_tasks (
            project_id TEXT NOT NULL,
            task_id INTEGER NOT NULL,
            intent TEXT NOT NULL,
            state TEXT NOT NULL,
            summary TEXT,
            error TEXT,
            completed_tool_calls INTEGER NOT NULL DEFAULT 0,
            updated_at_unix INTEGER NOT NULL,
            PRIMARY KEY (project_id, task_id)
        );

        CREATE TABLE IF NOT EXISTS agent_tool_calls (
            project_id TEXT NOT NULL,
            task_id INTEGER NOT NULL,
            tool_call_id TEXT NOT NULL,
            command_json TEXT NOT NULL,
            result_json TEXT NOT NULL,
            created_at_unix INTEGER NOT NULL,
            PRIMARY KEY (project_id, task_id, tool_call_id),
            FOREIGN KEY (project_id, task_id)
                REFERENCES agent_tasks(project_id, task_id)
        );

        CREATE TABLE IF NOT EXISTS agent_tool_errors (
            project_id TEXT NOT NULL,
            task_id INTEGER NOT NULL,
            tool_call_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            arguments_json TEXT NOT NULL,
            error TEXT NOT NULL,
            created_at_unix INTEGER NOT NULL,
            PRIMARY KEY (project_id, task_id, tool_call_id),
            FOREIGN KEY (project_id, task_id)
                REFERENCES agent_tasks(project_id, task_id)
        );

        CREATE TABLE IF NOT EXISTS agent_dispatch_tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            definition_id INTEGER NOT NULL,
            node_id INTEGER NOT NULL,
            original_instruction TEXT NOT NULL,
            task_text TEXT NOT NULL,
            state TEXT NOT NULL,
            tracking TEXT NOT NULL,
            error TEXT,
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL,
            FOREIGN KEY (definition_id) REFERENCES agent_definitions(id)
        );

        CREATE INDEX IF NOT EXISTS idx_agent_dispatch_tasks_definition_state
        ON agent_dispatch_tasks(definition_id, state, id);
        "#,
    )?;
    ensure_column(
        connection,
        "session_checkpoints",
        "cols",
        "cols INTEGER NOT NULL DEFAULT 80",
    )?;
    ensure_column(
        connection,
        "session_checkpoints",
        "rows",
        "rows INTEGER NOT NULL DEFAULT 24",
    )?;
    ensure_column(connection, "session_checkpoints", "shell", "shell TEXT")?;
    ensure_column(
        connection,
        "session_checkpoints",
        "agent_profile",
        "agent_profile TEXT",
    )?;
    ensure_column(
        connection,
        "voice_history",
        "raw_transcript",
        "raw_transcript TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "voice_history",
        "refined_transcript",
        "refined_transcript TEXT",
    )?;
    ensure_column(connection, "agent_definitions", "name_key", "name_key TEXT")?;
    ensure_column(
        connection,
        "agent_definitions",
        "speech_alias",
        "speech_alias TEXT",
    )?;
    connection.execute(
        "UPDATE agent_definitions SET name_key = lower(name) WHERE name_key IS NULL",
        [],
    )?;
    connection.execute(
        "UPDATE agent_definitions AS owner SET speech_alias = NULL
         WHERE speech_alias IS NOT NULL AND EXISTS (
             SELECT 1 FROM agent_definitions AS other
             WHERE other.id != owner.id AND other.name_key = owner.speech_alias
         )",
        [],
    )?;
    for builtin in agent::builtin_agent_names() {
        connection.execute(
            "UPDATE agent_definitions SET speech_alias = ?1 \
             WHERE name = ?2 COLLATE NOCASE AND speech_alias IS NULL \
             AND NOT EXISTS ( \
                 SELECT 1 FROM agent_definitions AS other \
                 WHERE other.id != agent_definitions.id \
                 AND (other.name_key = ?1 OR other.speech_alias = ?1) \
             )",
            rusqlite::params![builtin.speech_alias(), builtin.name()],
        )?;
    }
    connection.execute_batch(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_definitions_name_key
        ON agent_definitions(name_key);

        CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_definitions_speech_alias
        ON agent_definitions(speech_alias) WHERE speech_alias IS NOT NULL;
        "#,
    )?;
    connection.pragma_update(None, "user_version", 1)?;
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &'static str,
    column: &'static str,
    definition: &'static str,
) -> StorageResult<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(());
        }
    }

    connection.execute(&format!("ALTER TABLE {table} ADD COLUMN {definition}"), [])?;
    Ok(())
}

pub(crate) fn require_workspace_dir(path: &Path) -> Result<(), StorageError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(StorageError::InvalidWorkspacePath(path.to_path_buf()))
    }
}

#[cfg(test)]
#[path = "../tests/unit/migrations_tests.rs"]
mod tests;
