use crate::StorageResult;
use rusqlite::Connection;

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_agent_roles (
            workspace_id TEXT NOT NULL,
            card_instance_id TEXT NOT NULL,
            area_id TEXT NOT NULL,
            node_id INTEGER NOT NULL CHECK(node_id > 0),
            name TEXT NOT NULL,
            speech_alias TEXT,
            enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL,
            PRIMARY KEY(workspace_id,card_instance_id),
            UNIQUE(workspace_id,area_id,node_id)
        );
        CREATE TABLE IF NOT EXISTS workspace_agent_call_keys (
            workspace_id TEXT NOT NULL,
            normalized_key TEXT NOT NULL,
            card_instance_id TEXT NOT NULL,
            display TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('name','speech_alias')),
            state TEXT NOT NULL CHECK(state IN ('active','inactive')),
            PRIMARY KEY(workspace_id,normalized_key),
            FOREIGN KEY(workspace_id,card_instance_id)
                REFERENCES workspace_agent_roles(workspace_id,card_instance_id)
                ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS workspace_agent_role_creation (
            workspace_id TEXT NOT NULL, card_instance_id TEXT NOT NULL, owner_pid INTEGER NOT NULL,
            PRIMARY KEY(workspace_id,card_instance_id),
            FOREIGN KEY(workspace_id,card_instance_id)
                REFERENCES workspace_agent_roles(workspace_id,card_instance_id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS workspace_agent_role_repairs (
            workspace_id TEXT NOT NULL, card_instance_id TEXT NOT NULL,
            previous_name TEXT, name TEXT NOT NULL, repaired_at_unix INTEGER NOT NULL,
            PRIMARY KEY(workspace_id,card_instance_id),
            FOREIGN KEY(workspace_id,card_instance_id)
                REFERENCES workspace_agent_roles(workspace_id,card_instance_id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS workspace_agent_role_revisions (
            workspace_id TEXT PRIMARY KEY NOT NULL,
            revision INTEGER NOT NULL CHECK(revision > 0)
        );",
    )?;
    Ok(())
}
