use crate::{current_unix_timestamp, migrations::open_project_connection, StorageResult};
use canvas::DocumentSnapshot;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
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

    /// Background restore/registration only: persist legacy card identities before publishing.
    /// Query-only consumers use a separate readonly reader, never this migration API.
    pub fn load_snapshot_with_card_identity(&self) -> StorageResult<Option<DocumentSnapshot>> {
        // Background restore/directory IO only. The transaction prevents concurrent legacy
        // readers from publishing different identities for the same persisted card.
        let mut connection = open_project_connection(&self.path)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let json = transaction
            .query_row(
                "SELECT json FROM canvas_snapshots WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(json) = json else {
            transaction.commit()?;
            return Ok(None);
        };
        let mut snapshot: DocumentSnapshot = serde_json::from_str(&json)?;
        if snapshot.assign_missing_card_instance_ids() {
            transaction.execute(
                "UPDATE canvas_snapshots SET json = ?1 WHERE id = 1",
                [serde_json::to_string(&snapshot)?],
            )?;
        }
        transaction.commit()?;
        Ok(Some(snapshot))
    }
}

#[cfg(test)]
#[path = "../tests/unit/canvas_store_tests.rs"]
mod tests;
