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

    /// Background verification without creating a missing canvas file.
    pub fn contains_card_instance(&self, id: u64, instance: &str) -> StorageResult<bool> {
        if !self.path.try_exists()? {
            return Ok(false);
        }
        let c = rusqlite::Connection::open_with_flags(
            &self.path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let json: Option<String> = c
            .query_row("SELECT json FROM canvas_snapshots WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(json
            .map(|j| serde_json::from_str::<DocumentSnapshot>(&j))
            .transpose()?
            .is_some_and(|s| {
                s.nodes
                    .iter()
                    .any(|n| n.id == id && n.card_instance_id == instance)
            }))
    }

    /// Background rollback/deletion touches only the original identity and never recreates a missing DB.
    pub fn remove_card_instance(&self, id: u64, instance: &str) -> StorageResult<()> {
        if !self.path.try_exists()? {
            return Ok(());
        }
        let mut c = open_project_connection(&self.path)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let json: Option<String> = tx
            .query_row("SELECT json FROM canvas_snapshots WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(json) = json {
            let mut snapshot: DocumentSnapshot = serde_json::from_str(&json)?;
            let before = snapshot.nodes.len();
            snapshot
                .nodes
                .retain(|n| n.id != id || n.card_instance_id != instance);
            if snapshot.nodes.len() != before {
                record_card_removals(&tx, &[(id, instance.to_owned())])?;
                tx.execute(
                    "UPDATE canvas_snapshots SET json=?1 WHERE id=1",
                    [serde_json::to_string(&snapshot)?],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Append one reserved card to the latest row. Retries only accept the exact identity.
    pub fn insert_reserved_card(&self, node: &canvas::CanvasNodeSnapshot) -> StorageResult<()> {
        let mut c = open_project_connection(&self.path)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let json: Option<String> = tx
            .query_row("SELECT json FROM canvas_snapshots WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let mut snapshot: DocumentSnapshot = match json {
            Some(json) => serde_json::from_str(&json)?,
            None => canvas::Document::new().to_snapshot(),
        };
        if let Some(existing) = snapshot
            .nodes
            .iter()
            .find(|n| n.id == node.id || n.card_instance_id == node.card_instance_id)
        {
            if existing != node {
                return Err(crate::StorageError::WorkspaceAgentCardConflict(
                    node.card_instance_id.clone(),
                ));
            }
        } else {
            snapshot.nodes.push(node.clone());
        }
        tx.execute("INSERT INTO canvas_snapshots(id,json,updated_at_unix) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json,updated_at_unix=excluded.updated_at_unix",params![serde_json::to_string(&snapshot)?,current_unix_timestamp()?])?;
        tx.commit()?;
        Ok(())
    }

    /// Explicit user deletion proof commits with the canvas, for safe AppDB cleanup after a crash.
    pub fn save_snapshot_with_card_removals(
        &self,
        snapshot: &DocumentSnapshot,
        removed: &[(u64, String)],
    ) -> StorageResult<()> {
        if removed.iter().any(|(id, instance)| {
            snapshot
                .nodes
                .iter()
                .any(|n| n.id == *id && n.card_instance_id == *instance)
        }) {
            return Err(crate::StorageError::InvalidWorkspaceAgentRole(
                "card_not_removed".into(),
            ));
        }
        let mut c = open_project_connection(&self.path)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        record_card_removals(&tx, removed)?;
        tx.execute("INSERT INTO canvas_snapshots(id,json,updated_at_unix) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json,updated_at_unix=excluded.updated_at_unix",params![serde_json::to_string(snapshot)?,current_unix_timestamp()?])?;
        tx.commit()?;
        Ok(())
    }

    pub fn card_removal_recorded(&self, id: u64, instance: &str) -> StorageResult<bool> {
        if !self.path.try_exists()? {
            return Ok(false);
        }
        let c = rusqlite::Connection::open_with_flags(
            &self.path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let present:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='canvas_card_removals' AND type='table')",[],|r|r.get(0))?;
        if !present {
            return Ok(false);
        }
        Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM canvas_card_removals WHERE node_id=?1 AND card_instance_id=?2)",params![id,instance],|r|r.get(0))?)
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

    /// Patches only matching stable cards inside the latest row; layout and other nodes survive.
    pub fn patch_agent_aliases(&self, aliases: &[(u64, String, String)]) -> StorageResult<()> {
        let mut c = open_project_connection(&self.path)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let json: Option<String> = tx
            .query_row("SELECT json FROM canvas_snapshots WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(json) = json {
            let mut snapshot: DocumentSnapshot = serde_json::from_str(&json)?;
            let mut changed = false;
            for (id, instance, name) in aliases {
                if let Some(node) = snapshot
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == *id && n.card_instance_id == *instance)
                {
                    if let canvas::CanvasNodeKind::Agent(spec) = &mut node.kind {
                        if spec.alias.as_deref() != Some(name.as_str()) {
                            spec.alias = Some(name.clone());
                            changed = true;
                        }
                    }
                }
            }
            if changed {
                tx.execute(
                    "UPDATE canvas_snapshots SET json=?1 WHERE id=1",
                    [serde_json::to_string(&snapshot)?],
                )?;
            }
        }
        tx.commit()?;
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

fn record_card_removals(
    connection: &rusqlite::Connection,
    removed: &[(u64, String)],
) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS canvas_card_removals(node_id INTEGER NOT NULL,card_instance_id TEXT NOT NULL,PRIMARY KEY(node_id,card_instance_id))")?;
    for (id, instance) in removed {
        connection.execute(
            "INSERT OR IGNORE INTO canvas_card_removals(node_id,card_instance_id) VALUES(?1,?2)",
            params![id, instance],
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/canvas_store_tests.rs"]
mod tests;
