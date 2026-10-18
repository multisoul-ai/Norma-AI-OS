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
            added_at_unix_ms INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_desktop_project_areas_workspace
        ON desktop_project_areas(desktop_workspace_id, added_at_unix_ms);

        CREATE TABLE IF NOT EXISTS desktop_content_tabs (
            id TEXT PRIMARY KEY NOT NULL,
            workspace_id TEXT NOT NULL REFERENCES desktop_workspaces(id) ON DELETE CASCADE,
            source_area_id TEXT,
            source_project_name TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('browser', 'file')),
            resource TEXT NOT NULL,
            version TEXT NOT NULL DEFAULT '',
            title TEXT NOT NULL,
            position INTEGER NOT NULL,
            selected INTEGER NOT NULL DEFAULT 0 CHECK(selected IN (0, 1)),
            UNIQUE(workspace_id, kind, resource, version)
        );
        CREATE INDEX IF NOT EXISTS idx_desktop_content_tabs_workspace
        ON desktop_content_tabs(workspace_id, position);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_desktop_content_tabs_selected
        ON desktop_content_tabs(workspace_id) WHERE selected = 1;
        CREATE TABLE IF NOT EXISTS desktop_area_status (
            workspace_id TEXT NOT NULL REFERENCES desktop_workspaces(id) ON DELETE CASCADE,
            area_id TEXT NOT NULL REFERENCES desktop_project_areas(id) ON DELETE CASCADE,
            updated_at_unix_ms INTEGER NOT NULL,
            agents_json TEXT NOT NULL,
            PRIMARY KEY(workspace_id, area_id)
        );
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
    migrate_wallpapers(connection)?;
    migrate_project_instances(connection)?;
    Ok(())
}

fn migrate_project_instances(connection: &Connection) -> StorageResult<()> {
    if !has_unique_project(connection)? {
        return Ok(());
    }
    let transaction =
        rusqlite::Transaction::new_unchecked(connection, rusqlite::TransactionBehavior::Immediate)?;
    // Another background reader may have upgraded this catalog while we waited for the lock.
    if !has_unique_project(&transaction)? {
        return Ok(());
    }
    // Copy before dropping the catalog; IDs keep canvas files and expanded views intact.
    // Recreate the existing indexes/triggers only after copying, so revisions do not advance.
    let schema_objects = transaction
        .prepare("SELECT sql FROM sqlite_master WHERE tbl_name='desktop_project_areas' AND type IN ('index','trigger') AND sql IS NOT NULL")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    transaction.execute_batch(
        "CREATE TABLE desktop_project_areas_instances (
            id TEXT PRIMARY KEY NOT NULL,
            desktop_workspace_id TEXT NOT NULL REFERENCES desktop_workspaces(id) ON DELETE CASCADE,
            project_key TEXT NOT NULL,
            root_path TEXT NOT NULL,
            display_name TEXT NOT NULL,
            added_at_unix_ms INTEGER NOT NULL,
            wallpaper_json TEXT
        );
        INSERT INTO desktop_project_areas_instances
            SELECT id,desktop_workspace_id,project_key,root_path,display_name,added_at_unix_ms,wallpaper_json
            FROM desktop_project_areas;
        DROP TABLE desktop_project_areas;
        ALTER TABLE desktop_project_areas_instances RENAME TO desktop_project_areas;",
    )?;
    for sql in schema_objects {
        transaction.execute_batch(&sql)?;
    }
    transaction.commit()?;
    Ok(())
}

fn has_unique_project(connection: &Connection) -> StorageResult<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_index_list('desktop_project_areas') AS i
         WHERE i.\"unique\"=1 AND
         (SELECT group_concat(name, ',') FROM pragma_index_info(i.name))='desktop_workspace_id,project_key')",
        [],
        |row| row.get(0),
    ).map_err(Into::into)
}

fn migrate_wallpapers(connection: &Connection) -> StorageResult<()> {
    // Columns and backfill commit together; the marker prevents future global changes
    // from overwriting independent selections, including intentionally empty values.
    let transaction = connection.unchecked_transaction()?;
    super::ensure_column(
        &transaction,
        "desktop_workspaces",
        "wallpaper_json",
        "wallpaper_json TEXT",
    )?;
    super::ensure_column(
        &transaction,
        "desktop_project_areas",
        "wallpaper_json",
        "wallpaper_json TEXT",
    )?;
    let migrated: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key='wallpaper.desktop_migrated.v1')",
        [],
        |row| row.get(0),
    )?;
    if !migrated {
        if let Some(reference) =
            crate::desktop_workspace_store::wallpaper::legacy_reference(&transaction)?
        {
            transaction.execute(
                "UPDATE desktop_workspaces SET wallpaper_json=?1 WHERE wallpaper_json IS NULL",
                [serde_json::to_string(&reference)?],
            )?;
        }
        transaction.execute("INSERT INTO settings(key,value,updated_at_unix) VALUES('wallpaper.desktop_migrated.v1','1',0)", [])?;
    }
    transaction.commit()?;
    Ok(())
}
