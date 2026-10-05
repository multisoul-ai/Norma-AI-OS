use crate::{
    migrations::open_app_connection, MobileTurnKey, NewMobileCommand, StorageError, StorageResult,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};
use workspace::review::{ProjectReview, ReviewError};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskBaseline {
    Available { review: ProjectReview },
    Unavailable { reason: ReviewError },
}
impl TaskBaseline {
    fn validate(&self) -> StorageResult<()> {
        if let Self::Available { review } = self {
            review
                .validate()
                .map_err(|_| invalid("invalid task baseline"))?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct StoredTaskBaseline {
    pub workspace_id: String,
    pub route_id: String,
    pub device_id: String,
    pub operation_id: String,
    pub key: MobileTurnKey,
    pub baseline: TaskBaseline,
    pub recorded_at_unix: u64,
}

#[derive(Clone)]
pub struct MobileTaskBaselineStore {
    path: PathBuf,
}
impl MobileTaskBaselineStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Background-only metadata lookup. Never deserialize the baseline's file bytes for polling.
    pub fn turn_for_command(
        &self,
        device: &str,
        operation: &str,
        workspace: &str,
        route: &str,
    ) -> StorageResult<Option<MobileTurnKey>> {
        crate::mobile_turn_store::validate_workspace_scope(workspace)?;
        for id in [device, operation, route] {
            crate::mobile_access_store::validate_id(id)?;
        }
        let connection = open_app_connection(&self.path)?;
        let raw: Option<String> = connection.query_row(
            "SELECT json_object('key',json_extract(b.data_json,'$.key'),
              'workspace',json_extract(b.data_json,'$.workspace_id'),
              'route',json_extract(b.data_json,'$.route_id'),
              'device',json_extract(b.data_json,'$.device_id'),
              'operation',json_extract(b.data_json,'$.operation_id'),
              'run',b.run_id,'turn',b.turn_id)
             FROM mobile_task_baselines b JOIN mobile_commands c
               ON c.device_id=b.device_id AND c.operation_id=b.operation_id AND c.workspace_id=b.workspace_id
             WHERE b.device_id=?1 AND b.operation_id=?2 AND b.workspace_id=?3 AND c.route_id=?4",
            params![device, operation, workspace, route], |row| row.get(0),
        ).optional()?;
        raw.map(|raw| {
            let saved: serde_json::Value = serde_json::from_str(&raw)?;
            let key: MobileTurnKey = serde_json::from_value(saved["key"].clone())?;
            key.validate()?;
            if saved["workspace"] != workspace
                || saved["route"] != route
                || saved["device"] != device
                || saved["operation"] != operation
                || saved["run"] != key.run_id
                || saved["turn"] != key.turn_id
            {
                return Err(invalid("command turn scope mismatch"));
            }
            Ok(key)
        })
        .transpose()
    }

    /// Save the pre-send bytes only after the real Provider has confirmed its precise turn identity.
    /// An unconfirmed or deferred queue attempt never replaces a previously bound task baseline.
    pub fn record(
        &self,
        source: &NewMobileCommand,
        key: &MobileTurnKey,
        baseline: &TaskBaseline,
    ) -> StorageResult<StoredTaskBaseline> {
        key.validate()?;
        crate::mobile_turn_store::validate_workspace_scope(&source.workspace_id)?;
        baseline.validate()?;
        for id in [&source.device_id, &source.operation_id, &source.route_id] {
            crate::mobile_access_store::validate_id(id)?;
        }
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let matches: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM mobile_commands WHERE device_id=?1 AND operation_id=?2 AND workspace_id=?3 AND route_id=?4 AND owner_id=?5 AND request_hash=?6)", params![source.device_id, source.operation_id, source.workspace_id, source.route_id, source.owner_id, source.request_hash], |r| r.get(0))?;
        if !matches {
            return Err(invalid("task source does not match reserved command"));
        }
        if let Some(saved) = read(&tx, &source.workspace_id, key)? {
            if saved.key == *key
                && saved.device_id == source.device_id
                && saved.operation_id == source.operation_id
                && saved.route_id == source.route_id
                && saved.baseline == *baseline
            {
                return Ok(saved);
            }
            return Err(invalid("task baseline is immutable"));
        }
        let saved = StoredTaskBaseline {
            workspace_id: source.workspace_id.clone(),
            route_id: source.route_id.clone(),
            device_id: source.device_id.clone(),
            operation_id: source.operation_id.clone(),
            key: key.clone(),
            baseline: baseline.clone(),
            recorded_at_unix: crate::current_unix_timestamp()?,
        };
        let json = serde_json::to_string(&saved)?;
        //8MiB content is already validated; cap the expanded JSON too, matching the existing transport ceiling.
        if json.len() > 32 * 1024 * 1024 {
            return Err(invalid("task baseline serialization too large"));
        }
        tx.execute("INSERT INTO mobile_task_baselines(workspace_id,run_id,turn_id,device_id,operation_id,data_json) VALUES(?1,?2,?3,?4,?5,?6)", params![source.workspace_id,key.run_id,key.turn_id,source.device_id,source.operation_id,json])?;
        tx.commit()?;
        Ok(saved)
    }

    pub fn get(
        &self,
        workspace: &str,
        key: &MobileTurnKey,
    ) -> StorageResult<Option<StoredTaskBaseline>> {
        crate::mobile_turn_store::validate_workspace_scope(workspace)?;
        key.validate()?;
        let connection = open_app_connection(&self.path)?;
        Ok(read(&connection, workspace, key)?.filter(|saved| saved.key == *key))
    }
}

fn read(
    connection: &Connection,
    workspace: &str,
    key: &MobileTurnKey,
) -> StorageResult<Option<StoredTaskBaseline>> {
    let raw: Option<String> = connection.query_row("SELECT data_json FROM mobile_task_baselines WHERE workspace_id=?1 AND run_id=?2 AND turn_id=?3", params![workspace,key.run_id,key.turn_id], |r| r.get(0)).optional()?;
    raw.map(|raw| {
        let saved: StoredTaskBaseline = serde_json::from_str(&raw)?;
        saved.key.validate()?;
        saved.baseline.validate()?;
        if saved.workspace_id != workspace
            || saved.key.run_id != key.run_id
            || saved.key.turn_id != key.turn_id
        {
            return Err(invalid("task baseline scope mismatch"));
        }
        Ok(saved)
    })
    .transpose()
}
pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_task_baselines (
        workspace_id TEXT NOT NULL, run_id TEXT NOT NULL, turn_id TEXT NOT NULL,
        device_id TEXT NOT NULL, operation_id TEXT NOT NULL, data_json TEXT NOT NULL,
        PRIMARY KEY(workspace_id,run_id,turn_id), UNIQUE(device_id,operation_id),
        FOREIGN KEY(device_id,operation_id) REFERENCES mobile_commands(device_id,operation_id)
    )",
    )?;
    Ok(())
}
fn invalid(message: &str) -> StorageError {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}
