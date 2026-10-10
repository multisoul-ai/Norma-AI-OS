mod desktop;
pub(crate) mod desktop_canvas;
use crate::{error::StorageError, StorageResult};
use rusqlite::Connection;
use std::path::Path;

pub(crate) fn open_app_connection(path: &Path) -> StorageResult<Connection> {
    let connection = open_connection(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
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
    crate::workspace_agent_role_store::migrate(connection)?;
    crate::workspace_chat_store::migrate(connection)?;
    crate::agent_creation_store::migrate(connection)?;
    crate::memory_access::journal::migrate(connection)?;
    crate::thread_message_store::migrate(connection)?;
    crate::mobile_session_store::migrate(connection)?;
    crate::mobile_turn_store::migrate(connection)?;
    crate::mobile_command_store::migrate(connection)?;
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

        CREATE TABLE IF NOT EXISTS orchestration_authorizations (
            provider TEXT PRIMARY KEY NOT NULL,
            token_hash TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            installed_version TEXT NOT NULL,
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL,
            revoked_at_unix INTEGER
        );

        CREATE TABLE IF NOT EXISTS workspace_identities (
            workspace_id TEXT PRIMARY KEY NOT NULL,
            canonical_root TEXT UNIQUE NOT NULL,
            display_name TEXT NOT NULL,
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_identities_canonical_root
        ON workspace_identities(canonical_root);

        CREATE TABLE IF NOT EXISTS orchestration_task_routes (
            task_ref TEXT PRIMARY KEY NOT NULL,
            operation_id TEXT UNIQUE NOT NULL,
            request_fingerprint TEXT NOT NULL,
            source_workspace_id TEXT NOT NULL,
            source_definition_id INTEGER NOT NULL,
            source_provider TEXT NOT NULL,
            source_node_id_at_dispatch INTEGER NOT NULL,
            target_workspace_id TEXT NOT NULL,
            parent_task_ref TEXT,
            root_task_ref TEXT NOT NULL,
            target_task_ref TEXT,
            target_definition_id INTEGER,
            target_name TEXT,
            target_node_id_at_dispatch INTEGER,
            state TEXT NOT NULL,
            tracking TEXT NOT NULL,
            result_json TEXT,
            task_result_json TEXT,
            error_code TEXT,
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL,
            terminal_at_unix INTEGER
        );

        CREATE INDEX IF NOT EXISTS idx_orchestration_task_routes_source_workspace
        ON orchestration_task_routes(source_workspace_id, created_at_unix);

        CREATE INDEX IF NOT EXISTS idx_orchestration_task_routes_target_workspace
        ON orchestration_task_routes(target_workspace_id, created_at_unix);

        CREATE INDEX IF NOT EXISTS idx_orchestration_task_routes_state
        ON orchestration_task_routes(state, updated_at_unix);

        CREATE INDEX IF NOT EXISTS idx_orchestration_task_routes_target_runtime
        ON orchestration_task_routes(target_workspace_id, target_task_ref);

        CREATE TABLE IF NOT EXISTS orchestration_task_operations (
            operation_id TEXT PRIMARY KEY NOT NULL,
            request_fingerprint TEXT NOT NULL,
            result_json TEXT,
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL
        );
        "#,
    )?;
    desktop::migrate(connection)?;
    crate::desktop_workspace_store::mobile_configuration::migrate(connection)?;
    ensure_column(
        connection,
        "orchestration_task_routes",
        "parent_task_ref",
        "parent_task_ref TEXT",
    )?;
    ensure_column(
        connection,
        "orchestration_task_routes",
        "root_task_ref",
        "root_task_ref TEXT",
    )?;
    ensure_column(
        connection,
        "orchestration_task_routes",
        "target_definition_id",
        "target_definition_id INTEGER",
    )?;
    ensure_column(
        connection,
        "orchestration_task_routes",
        "target_name",
        "target_name TEXT",
    )?;
    ensure_column(
        connection,
        "orchestration_task_routes",
        "target_node_id_at_dispatch",
        "target_node_id_at_dispatch INTEGER",
    )?;
    ensure_column(
        connection,
        "orchestration_task_routes",
        "task_result_json",
        "task_result_json TEXT",
    )?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_orchestration_task_routes_root
         ON orchestration_task_routes(root_task_ref, state);",
    )?;
    // Rewriting the same version still takes SQLite's write lock. Mobile reads reopen this
    // connection for each authorization check, including every conversation fragment.
    if connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))? != 1 {
        connection.pragma_update(None, "user_version", 1)?;
    }
    crate::mobile_access_store::migrate(connection)?;
    crate::mobile_push_store::migrate(connection)?;
    crate::mobile_activity_store::migrate(connection)?;
    crate::project_plan_store::migrate(connection)?;
    crate::mobile_task_baseline_store::migrate(connection)?;
    crate::mobile_task_delivery_store::migrate(connection)?;
    crate::mobile_command_store::migrate_delivery_acceptances(connection)?;
    crate::mobile_command_store::migrate_delivery_feedback(connection)?;
    Ok(())
}
pub fn migrate_project_db(connection: &Connection) -> StorageResult<()> {
    crate::mobile_session_store::migrate(connection)?;
    crate::mobile_turn_store::migrate(connection)?;
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS approval_audit (
            run_id TEXT NOT NULL,
            sequence INTEGER NOT NULL CHECK(sequence > 0),
            entry_json TEXT NOT NULL,
            recorded_at_unix INTEGER NOT NULL,
            PRIMARY KEY (run_id, sequence)
        );

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
            origin TEXT NOT NULL DEFAULT 'explicit',
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_definitions_name_nocase
        ON agent_definitions(name COLLATE NOCASE);

        CREATE TABLE IF NOT EXISTS agent_dispatch_tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            runtime_instance_id TEXT NOT NULL DEFAULT '',
            runtime_task_id INTEGER NOT NULL DEFAULT 0,
            task_ref TEXT NOT NULL DEFAULT '',
            root_task_ref TEXT NOT NULL DEFAULT '',
            parent_task_ref TEXT,
            source_definition_id INTEGER,
            source_node_id INTEGER,
            source_provider TEXT,
            definition_id INTEGER NOT NULL,
            node_id INTEGER NOT NULL,
            original_instruction TEXT NOT NULL,
            task_text TEXT NOT NULL,
            state TEXT NOT NULL,
            tracking TEXT NOT NULL,
            error TEXT,
            result_captured INTEGER NOT NULL DEFAULT 0,
            result_text TEXT,
            result_truncated INTEGER NOT NULL DEFAULT 0,
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL,
            FOREIGN KEY (definition_id) REFERENCES agent_definitions(id)
        );

        CREATE INDEX IF NOT EXISTS idx_agent_dispatch_tasks_definition_state
        ON agent_dispatch_tasks(definition_id, state, id);

        CREATE TABLE IF NOT EXISTS agent_conversation_tails (
            definition_id INTEGER NOT NULL,
            session_id TEXT NOT NULL,
            node_id INTEGER NOT NULL,
            agent_name TEXT NOT NULL,
            profile TEXT NOT NULL,
            last_user_message TEXT,
            last_agent_message TEXT,
            updated_at_unix INTEGER NOT NULL,
            availability TEXT NOT NULL,
            user_message_truncated INTEGER NOT NULL DEFAULT 0,
            agent_message_truncated INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (definition_id, session_id),
            FOREIGN KEY (definition_id) REFERENCES agent_definitions(id)
        );

        CREATE INDEX IF NOT EXISTS idx_agent_conversation_tails_node
        ON agent_conversation_tails(node_id);
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
    ensure_column(
        connection,
        "agent_definitions",
        "origin",
        "origin TEXT NOT NULL DEFAULT 'explicit'",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "runtime_instance_id",
        "runtime_instance_id TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "runtime_task_id",
        "runtime_task_id INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "task_ref",
        "task_ref TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "root_task_ref",
        "root_task_ref TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "parent_task_ref",
        "parent_task_ref TEXT",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "source_definition_id",
        "source_definition_id INTEGER",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "source_node_id",
        "source_node_id INTEGER",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "source_provider",
        "source_provider TEXT",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "result_captured",
        "result_captured INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "result_text",
        "result_text TEXT",
    )?;
    ensure_column(
        connection,
        "agent_dispatch_tasks",
        "result_truncated",
        "result_truncated INTEGER NOT NULL DEFAULT 0",
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_agent_dispatch_tasks_runtime_task
         ON agent_dispatch_tasks(runtime_task_id, id)",
        [],
    )?;
    connection.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_dispatch_tasks_task_ref
         ON agent_dispatch_tasks(task_ref) WHERE task_ref != ''",
        [],
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

pub(crate) fn ensure_column(
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
