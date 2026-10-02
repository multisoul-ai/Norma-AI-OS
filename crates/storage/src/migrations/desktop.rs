use crate::StorageResult;
use rusqlite::Connection;

pub(super) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS desktop_workspaces (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            created_at_unix_ms INTEGER NOT NULL,
            last_opened_at_unix_ms INTEGER NOT NULL,
            expanded_area_id TEXT,
            overview_scroll_y INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_desktop_workspaces_recent
        ON desktop_workspaces(last_opened_at_unix_ms DESC);
        CREATE TABLE IF NOT EXISTS desktop_project_areas (
            id TEXT PRIMARY KEY NOT NULL,
            desktop_workspace_id TEXT NOT NULL REFERENCES desktop_workspaces(id) ON DELETE CASCADE,
            project_key TEXT NOT NULL,
            root_path TEXT NOT NULL,
            display_name TEXT NOT NULL,
            added_at_unix_ms INTEGER NOT NULL,
            UNIQUE(desktop_workspace_id, project_key)
        );

        CREATE INDEX IF NOT EXISTS idx_desktop_project_areas_workspace
        ON desktop_project_areas(desktop_workspace_id, added_at_unix_ms);
    "#,
    )?;
    super::ensure_column(
        connection,
        "desktop_workspaces",
        "expanded_area_id",
        "expanded_area_id TEXT",
    )?;
    super::ensure_column(
        connection,
        "desktop_workspaces",
        "overview_scroll_y",
        "overview_scroll_y INTEGER NOT NULL DEFAULT 0",
    )?;
    Ok(())
}
