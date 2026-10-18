use crate::{current_unix_timestamp, migrations::open_project_connection, StorageResult};
use canvas::{BrowserSurfaceKind, CanvasNodeKind, DocumentSnapshot, NodeId};
use rusqlite::{params, OptionalExtension};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct CanvasStore {
    path: PathBuf,
}

impl CanvasStore {
    pub fn new(path: PathBuf) -> Self {
        CanvasStore { path }
    }

    pub fn save_snapshot(&self, snapshot: &DocumentSnapshot) -> StorageResult<()> {
        let connection = open_project_connection(&self.path)?;
        let json = serde_json::to_string(snapshot)?;
        connection.execute(
            r#"
            INSERT INTO canvas_snapshots (id, json, updated_at_unix)
            VALUES (1, ?1, ?2)
            ON CONFLICT(id) DO UPDATE SET
                json = excluded.json,
                updated_at_unix = excluded.updated_at_unix
            "#,
            params![json, current_unix_timestamp()?],
        )?;
        Ok(())
    }

    pub fn load_snapshot(&self) -> StorageResult<Option<DocumentSnapshot>> {
        let connection = open_project_connection(&self.path)?;
        let json = connection
            .query_row(
                "SELECT json FROM canvas_snapshots WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        json.map(|value| Ok(serde_json::from_str(&value)?))
            .transpose()
    }

    /// Run on a background loader before restoring a desktop project session.
    /// The migration changes only the saved canvas record; it never touches files or browser data.
    pub fn prune_desktop_content_cards(&self) -> StorageResult<Vec<NodeId>> {
        let mut connection = open_project_connection(&self.path)?;
        let transaction = connection.transaction()?;
        let json: Option<String> = transaction
            .query_row("SELECT json FROM canvas_snapshots WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()?;
        let Some(json) = json else {
            return Ok(Vec::new());
        };
        let mut snapshot: DocumentSnapshot = serde_json::from_str(&json)?;
        let mut removed = Vec::new();
        snapshot.nodes.retain(|node| {
            let ordinary_content = match &node.kind {
                CanvasNodeKind::Preview(_) => true,
                CanvasNodeKind::Browser(spec) => {
                    matches!(&spec.surface, BrowserSurfaceKind::Native)
                }
                _ => false,
            };
            if ordinary_content {
                removed.push(node.id);
            }
            !ordinary_content
        });
        if !removed.is_empty() {
            transaction.execute(
                "UPDATE canvas_snapshots SET json=?1,updated_at_unix=?2 WHERE id=1",
                params![serde_json::to_string(&snapshot)?, current_unix_timestamp()?],
            )?;
        }
        transaction.commit()?;
        Ok(removed)
    }
}

#[cfg(test)]
#[path = "../tests/unit/canvas_store_tests.rs"]
mod tests;
