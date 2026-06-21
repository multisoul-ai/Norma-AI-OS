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
#[path = "migrations_tests.rs"]
mod tests;
