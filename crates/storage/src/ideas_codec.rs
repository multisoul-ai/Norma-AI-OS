//! 想法表的建表与行编解码，和状态流转分开维护。

use crate::ideas_store::invalid;
use crate::{StorageResult, StoredIdea};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use workspace::ideas::{IdeaSource, IdeaState};

pub(super) const SELECT: &str = "SELECT id,workspace_id,body,source,state,task_ref,assignee_area_id,assignee_node_id,position,created_at_unix,updated_at_unix,images FROM workspace_ideas";

type Raw = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
    i64,
    i64,
    i64,
    String,
);

pub(super) fn decode(row: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
    ))
}

pub(super) fn convert(raw: Raw) -> StorageResult<StoredIdea> {
    let (
        id,
        workspace_id,
        body,
        source,
        state,
        task_ref,
        area,
        node,
        position,
        created,
        updated,
        images,
    ) = raw;
    Ok(StoredIdea {
        source: IdeaSource::parse(&source).ok_or_else(|| invalid("unknown source"))?,
        state: IdeaState::parse(&state).ok_or_else(|| invalid("unknown state"))?,
        id,
        workspace_id,
        body,
        task_ref,
        assignee_area_id: area,
        assignee_node_id: node.map(|value| value as u64),
        position,
        images: crate::ideas_images::decode_names(&images),
        created_at_unix: created as u64,
        updated_at_unix: updated as u64,
    })
}

pub(super) fn read(
    tx: &Transaction<'_>,
    workspace_id: &str,
    id: &str,
) -> StorageResult<Option<StoredIdea>> {
    tx.query_row(
        &format!("{SELECT} WHERE workspace_id=?1 AND id=?2"),
        params![workspace_id, id],
        decode,
    )
    .optional()?
    .map(convert)
    .transpose()
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_ideas (
            id TEXT PRIMARY KEY NOT NULL,
            workspace_id TEXT NOT NULL REFERENCES desktop_workspaces(id) ON DELETE CASCADE,
            body TEXT NOT NULL,
            source TEXT NOT NULL,
            state TEXT NOT NULL,
            task_ref TEXT,
            assignee_area_id TEXT,
            assignee_node_id INTEGER,
            position INTEGER NOT NULL,
            created_at_unix INTEGER NOT NULL,
            updated_at_unix INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_ideas_workspace
        ON workspace_ideas(workspace_id, state, position);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_ideas_task_ref
        ON workspace_ideas(task_ref) WHERE task_ref IS NOT NULL;",
    )?;
    crate::migrations::ensure_column(
        connection,
        "workspace_ideas",
        "images",
        "images TEXT NOT NULL DEFAULT ''",
    )?;
    Ok(())
}
