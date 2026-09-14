use crate::{
    migrations::open_app_connection, MobileTurnKey, MobileTurnOutcome, StorageError, StorageResult,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io, path::PathBuf};
use workspace::review::ProjectReview;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskDeliveryContent {
    Available { review: ProjectReview },
    Unavailable { reason: String },
}
impl TaskDeliveryContent {
    fn validate(&self) -> StorageResult<()> {
        match self {
            Self::Available { review } => review
                .validate()
                .map_err(|_| invalid("invalid delivery content")),
            Self::Unavailable { reason }
                if matches!(
                    reason.as_str(),
                    "baseline_missing"
                        | "baseline_unavailable"
                        | "capture_expired"
                        | "capture_too_large"
                        | "capture_unsafe_path"
                        | "capture_changed"
                        | "capture_unavailable"
                        | "capture_unsupported"
                        | "task_changed"
                        | "window_closed"
                ) =>
            {
                Ok(())
            }
            Self::Unavailable { .. } => Err(invalid("invalid delivery failure")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoredTaskDelivery {
    pub workspace_id: String,
    pub outcome: MobileTurnOutcome,
    pub content: TaskDeliveryContent,
    pub fingerprint: String,
    pub saved_at_unix: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskDeliverySummary {
    pub sequence: u64,
    pub workspace_id: String,
    pub outcome: MobileTurnOutcome,
    pub fingerprint: String,
    pub saved_at_unix: u64,
    pub status: String,
    pub file_count: usize,
    #[serde(default)]
    pub acceptance: Option<crate::DeliveryAcceptance>,
}

#[derive(Clone)]
pub struct MobileTaskDeliveryStore {
    path: PathBuf,
}
impl MobileTaskDeliveryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Snapshot terminal, phone-origin turns whose original route expired before capture.
    /// The caller must verify the current route AFTER this read, before freezing unavailable
    /// records: a route created later must never be mistaken for an expired one.
    pub fn recovery_candidates(
        &self,
        workspace: &str,
        current_route: &str,
    ) -> StorageResult<Vec<MobileTurnKey>> {
        crate::mobile_access_store::validate_id(workspace)?;
        crate::mobile_access_store::validate_id(current_route)?;
        let connection = open_app_connection(&self.path)?;
        // Filter by the command's indexed identity before extracting any baseline metadata.
        // Never deserialize baseline file bytes or inspect the current project directory.
        let mut query = connection.prepare(
            "SELECT o.outcome_json, json_extract(b.data_json,'$.key')
             FROM mobile_task_baselines b
             JOIN mobile_commands c ON c.device_id=b.device_id AND c.operation_id=b.operation_id
             JOIN mobile_turn_outcomes o ON o.workspace_id=b.workspace_id AND o.run_id=b.run_id AND o.turn_id=b.turn_id
             LEFT JOIN mobile_task_deliveries d ON d.workspace_id=b.workspace_id AND d.run_id=b.run_id AND d.turn_id=b.turn_id
             WHERE b.workspace_id=?1 AND c.workspace_id=?1 AND c.route_id<>?2 AND d.run_id IS NULL",
        )?;
        let rows = query.query_map(params![workspace, current_route], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut keys = Vec::new();
        for row in rows {
            let (outcome, baseline) = row?;
            let outcome: MobileTurnOutcome = serde_json::from_str(&outcome)?;
            let baseline: MobileTurnKey = serde_json::from_str(&baseline)?;
            outcome.key.validate()?;
            baseline.validate()?;
            if outcome.key == baseline {
                keys.push(outcome.key);
            }
        }
        Ok(keys)
    }

    /// Freeze once, only after the actual Provider outcome is durable. Refresh never substitutes later bytes.
    pub fn record(
        &self,
        workspace: &str,
        key: &MobileTurnKey,
        content: &TaskDeliveryContent,
    ) -> StorageResult<StoredTaskDelivery> {
        crate::mobile_access_store::validate_id(workspace)?;
        key.validate()?;
        content.validate()?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let outcome = crate::mobile_turn_store::read(&tx, workspace, &key.run_id, &key.turn_id)?
            .filter(|o| o.key == *key)
            .ok_or_else(|| invalid("delivery requires the precise durable turn outcome"))?;
        if let Some(saved) = read(&tx, workspace, key)? {
            if saved.outcome == outcome && saved.content == *content {
                return Ok(saved);
            }
            return Err(invalid("task delivery is immutable"));
        }
        let saved = StoredTaskDelivery {
            workspace_id: workspace.into(),
            fingerprint: fingerprint(workspace, &outcome, content)?,
            outcome,
            content: content.clone(),
            saved_at_unix: crate::current_unix_timestamp()?,
        };
        let json = serde_json::to_string(&saved)?;
        //8MiB validated bytes can expand in JSON;32MiB matches the existing bounded transfer ceiling.
        if json.len() > 32 * 1024 * 1024 {
            return Err(invalid("delivery serialization too large"));
        }
        tx.execute("INSERT INTO mobile_task_deliveries(workspace_id,run_id,turn_id,data_json,node_id) VALUES(?1,?2,?3,?4,?5)",params![workspace,key.run_id,key.turn_id,json,key.node_id])?;
        tx.commit()?;
        Ok(saved)
    }
    /// Bounded metadata pages; file bytes are not deserialized into Rust or sent in the list.
    pub fn list(
        &self,
        workspace: &str,
        node: Option<u64>,
        before: Option<u64>,
    ) -> StorageResult<Vec<TaskDeliverySummary>> {
        self.page(workspace, node, before, false)
            .map(|(items, _)| items)
    }
    /// Count the whole inbox and page metadata in one read snapshot. No file bytes leave SQLite.
    pub fn page(
        &self,
        workspace: &str,
        node: Option<u64>,
        before: Option<u64>,
        pending_only: bool,
    ) -> StorageResult<(Vec<TaskDeliverySummary>, u64)> {
        crate::mobile_access_store::validate_id(workspace)?;
        if node.is_some_and(|n| n == 0 || n > i64::MAX as u64)
            || before.is_some_and(|n| n == 0 || n > i64::MAX as u64)
        {
            return Err(invalid("invalid delivery cursor or node"));
        }
        let mut connection = open_app_connection(&self.path)?;
        let connection = connection.transaction()?;
        // Keep both scopes indexable: no optional-node OR that forces scanning other cards.
        let node_filter = if node.is_some() {
            "d.node_id=?2"
        } else {
            "?2 IS NULL"
        };
        let inbox = format!("{REVIEWABLE} AND a.run_id IS NULL");
        let filter = if pending_only { inbox.as_str() } else { "1" };
        let mut query=connection.prepare(&format!("SELECT json_object('sequence',d.sequence,'workspace_id',d.workspace_id,
            'outcome',json_extract(d.data_json,'$.outcome'), 'fingerprint',json_extract(d.data_json,'$.fingerprint'),
            'saved_at_unix',json_extract(d.data_json,'$.saved_at_unix'),
            'status',coalesce(json_extract(d.data_json,'$.content.reason'),json_extract(d.data_json,'$.content.status')),
            'file_count',coalesce(json_array_length(d.data_json,'$.content.review.files'),0),
            'acceptance',json(a.acceptance_json))
            FROM mobile_task_deliveries d LEFT JOIN mobile_delivery_acceptances a
              ON a.workspace_id=d.workspace_id AND a.run_id=d.run_id AND a.turn_id=d.turn_id
            WHERE d.workspace_id=?1 AND {node_filter} AND {filter} AND d.sequence < ?3 ORDER BY d.sequence DESC LIMIT 20"))?;
        let rows = query.query_map(
            params![workspace, node, before.unwrap_or(i64::MAX as u64)],
            |row| row.get::<_, String>(0),
        )?;
        let items = rows
            .map(|raw| {
                let summary: TaskDeliverySummary = serde_json::from_str(&raw?)?;
                summary.outcome.key.validate()?;
                if summary.workspace_id != workspace
                    || node.is_some_and(|n| summary.outcome.key.node_id != n)
                {
                    return Err(invalid("delivery summary scope mismatch"));
                }
                if let Some(accepted) = &summary.acceptance {
                    accepted.validate()?;
                    if accepted.workspace_id != workspace
                        || accepted.key != summary.outcome.key
                        || accepted.fingerprint != summary.fingerprint
                        || summary.status != "available"
                        || summary.outcome.state != crate::MobileTurnState::Completed
                    {
                        return Err(invalid("delivery summary acceptance mismatch"));
                    }
                }
                Ok(summary)
            })
            .collect::<StorageResult<Vec<_>>>()?;
        // The partial index covers eligibility and run/turn join keys; counting never reads data_json.
        let count = connection.query_row(
            &format!(
                "SELECT count(*) FROM mobile_task_deliveries d
            LEFT JOIN mobile_delivery_acceptances a
              ON a.workspace_id=d.workspace_id AND a.run_id=d.run_id AND a.turn_id=d.turn_id
            WHERE d.workspace_id=?1 AND {node_filter} AND {inbox}"
            ),
            params![workspace, node],
            |row| row.get(0),
        )?;
        Ok((items, count))
    }
    pub fn get(
        &self,
        workspace: &str,
        key: &MobileTurnKey,
    ) -> StorageResult<Option<StoredTaskDelivery>> {
        crate::mobile_access_store::validate_id(workspace)?;
        key.validate()?;
        let connection = open_app_connection(&self.path)?;
        Ok(read(&connection, workspace, key)?.filter(|saved| saved.outcome.key == *key))
    }
}
pub(crate) fn read(
    connection: &Connection,
    workspace: &str,
    key: &MobileTurnKey,
) -> StorageResult<Option<StoredTaskDelivery>> {
    let raw:Option<String>=connection.query_row("SELECT data_json FROM mobile_task_deliveries WHERE workspace_id=?1 AND run_id=?2 AND turn_id=?3",params![workspace,key.run_id,key.turn_id],|r|r.get(0)).optional()?;
    raw.map(|raw| {
        if raw.len() > 32 * 1024 * 1024 {
            return Err(invalid("saved delivery too large"));
        }
        let saved: StoredTaskDelivery = serde_json::from_str(&raw)?;
        saved.outcome.key.validate()?;
        saved.content.validate()?;
        if saved.workspace_id != workspace
            || saved.outcome.key.run_id != key.run_id
            || saved.outcome.key.turn_id != key.turn_id
            || saved.fingerprint != fingerprint(workspace, &saved.outcome, &saved.content)?
        {
            return Err(invalid("saved delivery identity or content mismatch"));
        }
        Ok(saved)
    })
    .transpose()
}
fn fingerprint(
    workspace: &str,
    outcome: &MobileTurnOutcome,
    content: &TaskDeliveryContent,
) -> StorageResult<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(workspace, outcome, content))?)
    ))
}
// Exactly the same eligibility as accept_delivery: completed, frozen, and without conflicts.
const REVIEWABLE: &str = "json_extract(data_json,'$.content.status')='available' AND json_extract(data_json,'$.outcome.state')='completed' AND json_extract(data_json,'$.content.review.conflicted')=0";
pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS mobile_task_deliveries (
        sequence INTEGER PRIMARY KEY AUTOINCREMENT, node_id INTEGER NOT NULL,
        workspace_id TEXT NOT NULL,run_id TEXT NOT NULL,turn_id TEXT NOT NULL,data_json TEXT NOT NULL,
        UNIQUE(workspace_id,run_id,turn_id),
        FOREIGN KEY(workspace_id,run_id,turn_id) REFERENCES mobile_turn_outcomes(workspace_id,run_id,turn_id)
    ); CREATE INDEX IF NOT EXISTS mobile_task_delivery_history ON mobile_task_deliveries(workspace_id,node_id,sequence);
    CREATE INDEX IF NOT EXISTS mobile_task_delivery_project_history ON mobile_task_deliveries(workspace_id,sequence)")?;
    connection.execute_batch(&format!(
        "CREATE INDEX IF NOT EXISTS mobile_task_delivery_inbox
        ON mobile_task_deliveries(workspace_id,node_id,sequence,run_id,turn_id) WHERE {REVIEWABLE}"
    ))?;
    Ok(())
}
fn invalid(message: &str) -> StorageError {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}
