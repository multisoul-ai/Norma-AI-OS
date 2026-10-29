use crate::StorageResult;
use rusqlite::Connection;

/// Preserve existing tab IDs, selection and frozen versions while permitting explicit copies.
pub(super) fn migrate(connection: &Connection) -> StorageResult<()> {
    if super::content_schema_contains(connection, "instance_id")? {
        return Ok(());
    }
    let transaction =
        rusqlite::Transaction::new_unchecked(connection, rusqlite::TransactionBehavior::Immediate)?;
    if super::content_schema_contains(&transaction, "instance_id")? {
        return Ok(());
    }
    transaction.execute_batch("ALTER TABLE desktop_content_tabs RENAME TO desktop_content_tabs_before_present;
        CREATE TABLE desktop_content_tabs (
            id TEXT PRIMARY KEY NOT NULL,
            workspace_id TEXT NOT NULL REFERENCES desktop_workspaces(id) ON DELETE CASCADE,
            source_area_id TEXT, source_project_name TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('browser','file','project_files')),
            resource TEXT NOT NULL, version TEXT NOT NULL DEFAULT '', title TEXT NOT NULL,
            position INTEGER NOT NULL, selected INTEGER NOT NULL DEFAULT 0 CHECK(selected IN (0,1)),
            instance_id TEXT NOT NULL DEFAULT '', preview_type TEXT,
            UNIQUE(workspace_id,kind,resource,version,instance_id));
        INSERT INTO desktop_content_tabs(id,workspace_id,source_area_id,source_project_name,kind,resource,version,title,position,selected)
            SELECT id,workspace_id,source_area_id,source_project_name,kind,resource,version,title,position,selected
            FROM desktop_content_tabs_before_present;
        DROP TABLE desktop_content_tabs_before_present;
        CREATE INDEX idx_desktop_content_tabs_workspace ON desktop_content_tabs(workspace_id,position);
        CREATE UNIQUE INDEX idx_desktop_content_tabs_selected ON desktop_content_tabs(workspace_id) WHERE selected=1;
        ")?;
    transaction.commit()?;
    Ok(())
}
