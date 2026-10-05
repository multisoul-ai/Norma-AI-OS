//! Creation ledger is separate from chat messages. Claimed work is never automatically replayed.
use crate::{migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentCreationReceipt {
    pub device_id: String,
    pub mac_id: String,
    pub desktop_workspace_id: String,
    pub operation_id: String,
    pub state: String,
    pub result: Option<Value>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct AgentCreationStore {
    path: PathBuf,
}

impl AgentCreationStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn receive(
        &self,
        device: &str,
        mac: &str,
        workspace: &str,
        operation: &str,
        payload: &str,
    ) -> StorageResult<AgentCreationReceipt> {
        for id in [device, mac, workspace, operation] {
            crate::mobile_access_store::validate_id(id)?;
        }
        if payload.len() > 4096 || !serde_json::from_str::<Value>(payload)?.is_object() {
            return Err(invalid("invalid_request"));
        }
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<(String, String, String)> = tx.query_row("SELECT mac_id,workspace_id,payload FROM agent_creation_operations WHERE device_id=?1 AND operation_id=?2", params![device, operation], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some(previous) = previous {
            if previous != (mac.into(), workspace.into(), payload.into()) {
                return Err(invalid("idempotency_conflict"));
            }
        } else {
            tx.execute("INSERT INTO agent_creation_operations(device_id,operation_id,mac_id,workspace_id,payload,state) VALUES(?1,?2,?3,?4,?5,'received')",params![device,operation,mac,workspace,payload])?;
        }
        let result = read(&tx, device, mac, workspace, operation)?
            .ok_or_else(|| invalid("receipt_missing"))?;
        tx.commit()?;
        Ok(result)
    }

    pub fn receipt(
        &self,
        device: &str,
        mac: &str,
        workspace: &str,
        operation: &str,
    ) -> StorageResult<Option<AgentCreationReceipt>> {
        read(
            &open_app_connection(&self.path)?,
            device,
            mac,
            workspace,
            operation,
        )
    }

    pub fn claim(&self, device: &str, operation: &str) -> StorageResult<bool> {
        Ok(open_app_connection(&self.path)?.execute("UPDATE agent_creation_operations SET state='processing' WHERE device_id=?1 AND operation_id=?2 AND state='received'",params![device,operation])? == 1)
    }

    /// Record each actual identity before the next side effect; merge instead of erasing earlier bindings.
    pub fn bind(&self, device: &str, operation: &str, binding: &Value) -> StorageResult<()> {
        if !binding.is_object() {
            return Err(invalid("invalid_binding"));
        }
        let conn = open_app_connection(&self.path)?;
        let changed = conn.execute("UPDATE agent_creation_operations SET result=json_patch(COALESCE(result,'{}'),?3) WHERE device_id=?1 AND operation_id=?2 AND state='processing'",params![device,operation,binding.to_string()])?;
        if changed != 1 {
            return Err(invalid("creation_not_claimed"));
        }
        Ok(())
    }

    pub fn finish(
        &self,
        device: &str,
        operation: &str,
        state: &str,
        error: Option<&str>,
    ) -> StorageResult<()> {
        if !["succeeded", "failed", "outcome_unknown"].contains(&state) {
            return Err(invalid("invalid_state"));
        }
        open_app_connection(&self.path)?.execute("UPDATE agent_creation_operations SET state=?3,error=?4 WHERE device_id=?1 AND operation_id=?2 AND state='processing'",params![device,operation,state,error])?;
        Ok(())
    }

    pub fn reconcile_interrupted(&self) -> StorageResult<()> {
        open_app_connection(&self.path)?.execute("UPDATE agent_creation_operations SET state='outcome_unknown',error='mac_restarted' WHERE state='processing'",[])?;
        Ok(())
    }

    /// Only verified original card identities may close an interrupted creation. Never replays work.
    pub fn resolve_unknown(
        &self,
        device: &str,
        operation: &str,
        actual: &Value,
    ) -> StorageResult<()> {
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let binding: Option<String> = tx.query_row("SELECT result FROM agent_creation_operations WHERE device_id=?1 AND operation_id=?2 AND state='outcome_unknown'", params![device,operation], |r|r.get(0)).optional()?.flatten();
        let binding: Value =
            serde_json::from_str(&binding.ok_or_else(|| invalid("binding_missing"))?)?;
        for key in ["area_id", "definition_id", "node_id"] {
            if binding[key].is_null() || binding[key] != actual[key] {
                return Err(invalid("creation_identity_changed"));
            }
        }
        if actual["card"]["node_id"] != binding["node_id"] {
            return Err(invalid("creation_identity_changed"));
        }
        tx.execute("UPDATE agent_creation_operations SET state='succeeded',error=NULL,result=json_patch(result,?3) WHERE device_id=?1 AND operation_id=?2 AND state='outcome_unknown'",params![device,operation,actual.to_string()])?;
        tx.commit()?;
        Ok(())
    }
}

fn read(
    conn: &rusqlite::Connection,
    device: &str,
    mac: &str,
    workspace: &str,
    operation: &str,
) -> StorageResult<Option<AgentCreationReceipt>> {
    let row = conn.query_row("SELECT state,result,error FROM agent_creation_operations WHERE device_id=?1 AND operation_id=?2 AND mac_id=?3 AND workspace_id=?4",params![device,operation,mac,workspace],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<String>>(2)?))).optional()?;
    row.map(|(state, result, error)| {
        Ok(AgentCreationReceipt {
            device_id: device.into(),
            mac_id: mac.into(),
            desktop_workspace_id: workspace.into(),
            operation_id: operation.into(),
            state,
            result: result.map(|r| serde_json::from_str(&r)).transpose()?,
            error,
        })
    })
    .transpose()
}
fn invalid(code: &str) -> StorageError {
    StorageError::InvalidMobileCommand(code.into())
}
pub(crate) fn migrate(conn: &rusqlite::Connection) -> StorageResult<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS agent_creation_operations(device_id TEXT NOT NULL,operation_id TEXT NOT NULL,mac_id TEXT NOT NULL,workspace_id TEXT NOT NULL,payload TEXT NOT NULL,state TEXT NOT NULL,result TEXT,error TEXT,PRIMARY KEY(device_id,operation_id));")?;
    Ok(())
}
