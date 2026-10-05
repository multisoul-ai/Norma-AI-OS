//! Durable mobile workspace operations. Never replay a claimed operation after a crash.
use crate::{migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceChatReceipt {
    pub device_id: String,
    pub mac_id: String,
    pub desktop_workspace_id: String,
    pub operation_id: String,
    pub state: String,
    pub cursor: u64,
    pub result: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceChatEvent {
    pub mac_id: String,
    pub desktop_workspace_id: String,
    pub cursor: u64,
    pub device_id: String,
    pub operation_id: String,
    pub kind: String,
    pub created_at_unix_ms: i64,
    pub payload: serde_json::Value,
}

#[derive(Clone)]
pub struct WorkspaceChatStore {
    path: PathBuf,
}

impl WorkspaceChatStore {
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
    ) -> StorageResult<WorkspaceChatReceipt> {
        validate(device, mac, workspace, operation, payload)?;
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<(String, String, String)> = tx.query_row(
            "SELECT mac_id,workspace_id,payload FROM workspace_chat_operations WHERE device_id=?1 AND operation_id=?2",
            params![device, operation], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some(previous) = previous {
            if previous != (mac.into(), workspace.into(), payload.into()) {
                return Err(invalid("idempotency_conflict"));
            }
        } else {
            tx.execute("INSERT INTO workspace_chat_operations(device_id,operation_id,mac_id,workspace_id,payload,state) VALUES(?1,?2,?3,?4,?5,'received')", params![device,operation,mac,workspace,payload])?;
            tx.execute("INSERT INTO workspace_chat_events(device_id,operation_id,mac_id,workspace_id,kind,payload,created_at_unix_ms) VALUES(?1,?2,?3,?4,'received',?5,CAST(strftime('%s','now') AS INTEGER)*1000)",params![device,operation,mac,workspace,payload])?;
        }
        let receipt = read(&tx, device, mac, workspace, operation)?
            .ok_or_else(|| invalid("receipt_missing"))?;
        tx.commit()?;
        Ok(receipt)
    }

    pub fn receipt(
        &self,
        device: &str,
        mac: &str,
        workspace: &str,
        operation: &str,
    ) -> StorageResult<Option<WorkspaceChatReceipt>> {
        let conn = open_app_connection(&self.path)?;
        read(&conn, device, mac, workspace, operation)
    }

    pub fn claim(&self, device: &str, operation: &str) -> StorageResult<bool> {
        let conn = open_app_connection(&self.path)?;
        Ok(conn.execute("UPDATE workspace_chat_operations SET state='processing' WHERE device_id=?1 AND operation_id=?2 AND state='received'",params![device,operation])? == 1)
    }

    pub fn finish(
        &self,
        device: &str,
        operation: &str,
        state: &str,
        result: &str,
    ) -> StorageResult<()> {
        if !["succeeded", "failed", "outcome_unknown"].contains(&state) {
            return Err(invalid("invalid_state"));
        }
        let _: serde_json::Value = serde_json::from_str(result)?;
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = if state == "outcome_unknown" {
            let previous: Option<String> = tx.query_row("SELECT result FROM workspace_chat_operations WHERE device_id=?1 AND operation_id=?2", params![device,operation], |r| r.get(0)).optional()?.flatten();
            let mut merged: serde_json::Value =
                serde_json::from_str(previous.as_deref().unwrap_or("{}"))?;
            if let (Some(base), Some(update)) = (
                merged.as_object_mut(),
                serde_json::from_str::<serde_json::Value>(result)?.as_object(),
            ) {
                base.extend(update.clone());
            }
            merged.to_string()
        } else {
            result.to_owned()
        };
        let changed = tx.execute("UPDATE workspace_chat_operations SET state=?3,result=?4 WHERE device_id=?1 AND operation_id=?2 AND state='processing'",params![device,operation,state,result])?;
        if changed == 1 {
            tx.execute("INSERT INTO workspace_chat_events(device_id,operation_id,mac_id,workspace_id,kind,payload,created_at_unix_ms) SELECT device_id,operation_id,mac_id,workspace_id,?3,?4,CAST(strftime('%s','now') AS INTEGER)*1000 FROM workspace_chat_operations WHERE device_id=?1 AND operation_id=?2",params![device,operation,state,result])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn record_event(
        &self,
        device: &str,
        operation: &str,
        kind: &str,
        payload: &str,
    ) -> StorageResult<()> {
        let _: serde_json::Value = serde_json::from_str(payload)?;
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if kind == "task_result" {
            let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM workspace_chat_events WHERE device_id=?1 AND operation_id=?2 AND kind='task_result' AND COALESCE(json_extract(payload,'$.task_ref'),'')=COALESCE(json_extract(?3,'$.task_ref'),'') AND COALESCE(json_extract(payload,'$.turn_id'),'')=COALESCE(json_extract(?3,'$.turn_id'),''))",params![device,operation,payload],|r|r.get(0))?;
            if exists {
                return Ok(());
            }
        }
        tx.execute("INSERT INTO workspace_chat_events(device_id,operation_id,mac_id,workspace_id,kind,payload,created_at_unix_ms) SELECT device_id,operation_id,mac_id,workspace_id,?3,?4,CAST(strftime('%s','now') AS INTEGER)*1000 FROM workspace_chat_operations WHERE device_id=?1 AND operation_id=?2 ",params![device,operation,kind,payload])?;
        tx.execute("UPDATE workspace_chat_operations SET result=?3 WHERE device_id=?1 AND operation_id=?2 AND state='processing'",params![device,operation,payload])?;
        tx.commit()?;
        Ok(())
    }

    pub fn events(
        &self,
        mac: &str,
        workspace: &str,
        after: u64,
    ) -> StorageResult<Vec<WorkspaceChatEvent>> {
        let conn = open_app_connection(&self.path)?;
        let mut query = conn.prepare("SELECT sequence,device_id,operation_id,kind,payload,created_at_unix_ms FROM workspace_chat_events WHERE mac_id=?1 AND workspace_id=?2 AND sequence>?3 ORDER BY sequence LIMIT 32")?;
        let rows = query.query_map(params![mac, workspace, after], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?;
        // 160 KiB leaves 96 KiB for the encrypted envelope under the 256 KiB relay limit.
        let mut result = Vec::new();
        let mut bytes = 2;
        for row in rows {
            let (cursor, device_id, operation_id, kind, payload, created_at_unix_ms) = row?;
            let event = WorkspaceChatEvent {
                mac_id: mac.into(),
                desktop_workspace_id: workspace.into(),
                cursor,
                device_id,
                operation_id,
                kind,
                created_at_unix_ms,
                payload: serde_json::from_str(&payload)?,
            };
            let size = serde_json::to_vec(&event)?.len() + 1;
            if bytes + size > 160 * 1024 {
                break;
            }
            bytes += size;
            result.push(event);
        }
        Ok(result)
    }

    /// Recent source-linked facts for one accepted operation. Full history stays in the ledger.
    pub fn recent_context(
        &self,
        device: &str,
        operation: &str,
    ) -> StorageResult<serde_json::Value> {
        let conn = open_app_connection(&self.path)?;
        let mut query = conn.prepare("SELECT sequence,device_id,operation_id,kind,payload FROM workspace_chat_events e WHERE (mac_id,workspace_id)=(SELECT mac_id,workspace_id FROM workspace_chat_operations WHERE device_id=?1 AND operation_id=?2) AND NOT(device_id=?1 AND operation_id=?2) AND kind IN ('received','succeeded','failed','task_result','dispatch') ORDER BY sequence DESC LIMIT 48")?;
        let rows = query.query_map(params![device, operation], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut facts = Vec::new();
        let mut bytes = 2;
        for row in rows {
            let (cursor, device, operation, kind, payload) = row?;
            let payload: serde_json::Value = serde_json::from_str(&payload)?;
            let fact = serde_json::json!({"cursor":cursor,"device":device,"operation":operation,"kind":kind,"sender":payload["sender"],"text":payload["text"].as_str().unwrap_or_default().chars().take(2048).collect::<String>(),"target":payload["target"],"task_ref":payload["task_ref"],"state":payload["state"]});
            // 24 KiB ~= 6K ASCII tokens; old full messages remain retrievable by event cursor.
            let size = serde_json::to_vec(&fact)?.len() + 1;
            if bytes + size > 24 * 1024 {
                break;
            }
            bytes += size;
            facts.push(fact);
        }
        facts.reverse();
        Ok(serde_json::Value::Array(facts))
    }

    /// Submitted executions lacking a frozen result. Querying this never replays user input.
    pub fn pending_results(
        &self,
        mac: &str,
        workspace: &str,
    ) -> StorageResult<Vec<WorkspaceChatEvent>> {
        let conn = open_app_connection(&self.path)?;
        let mut query=conn.prepare("SELECT sequence,device_id,operation_id,COALESCE((SELECT binding.payload FROM workspace_chat_events binding WHERE binding.device_id=e.device_id AND binding.operation_id=e.operation_id AND binding.kind='running' AND json_extract(binding.payload,'$.task_ref')=json_extract(e.payload,'$.task_ref') ORDER BY binding.sequence DESC LIMIT 1),payload),e.created_at_unix_ms FROM workspace_chat_events e WHERE mac_id=?1 AND workspace_id=?2 AND kind='submitted' AND NOT EXISTS (SELECT 1 FROM workspace_chat_events done WHERE done.device_id=e.device_id AND done.operation_id=e.operation_id AND done.kind='task_result' AND COALESCE(json_extract(done.payload,'$.task_ref'),'')=COALESCE(json_extract(e.payload,'$.task_ref'),'') AND COALESCE(json_extract(done.payload,'$.turn_id'),'')=COALESCE(json_extract(e.payload,'$.turn_id'),'')) ORDER BY sequence LIMIT 32")?;
        let rows = query.query_map(params![mac, workspace], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?;
        rows.map(|r| {
            let (cursor, device_id, operation_id, payload, created_at_unix_ms) = r?;
            Ok(WorkspaceChatEvent {
                mac_id: mac.into(),
                desktop_workspace_id: workspace.into(),
                cursor,
                device_id,
                operation_id,
                kind: "submitted".into(),
                created_at_unix_ms,
                payload: serde_json::from_str(&payload)?,
            })
        })
        .collect()
    }

    /// Recovery reports uncertain work; it never resubmits input to an Agent.
    pub fn reconcile_interrupted(&self) -> StorageResult<()> {
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO workspace_chat_events(device_id,operation_id,mac_id,workspace_id,kind,payload,created_at_unix_ms) SELECT device_id,operation_id,mac_id,workspace_id,'outcome_unknown',json_set(COALESCE(result,'{}'),'$.error','mac_restarted'),CAST(strftime('%s','now') AS INTEGER)*1000 FROM workspace_chat_operations WHERE state='processing'",[])?;
        tx.execute("UPDATE workspace_chat_operations SET state='outcome_unknown',result=json_set(COALESCE(result,'{}'),'$.error','mac_restarted') WHERE state='processing'",[])?;
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
) -> StorageResult<Option<WorkspaceChatReceipt>> {
    let row = conn.query_row("SELECT state,(SELECT MAX(sequence) FROM workspace_chat_events e WHERE e.device_id=o.device_id AND e.operation_id=o.operation_id),result FROM workspace_chat_operations o WHERE device_id=?1 AND operation_id=?2 AND mac_id=?3 AND workspace_id=?4",params![device,operation,mac,workspace], |r| Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?,r.get::<_,Option<String>>(2)?))).optional()?;
    row.map(|(state, cursor, result)| {
        Ok(WorkspaceChatReceipt {
            device_id: device.into(),
            mac_id: mac.into(),
            desktop_workspace_id: workspace.into(),
            operation_id: operation.into(),
            state,
            cursor,
            result: result.map(|r| serde_json::from_str(&r)).transpose()?,
        })
    })
    .transpose()
}

fn validate(
    device: &str,
    mac: &str,
    workspace: &str,
    operation: &str,
    payload: &str,
) -> StorageResult<()> {
    for id in [device, mac, workspace, operation] {
        crate::mobile_access_store::validate_id(id)?;
    }
    if payload.len() > 192 * 1024
        || !serde_json::from_str::<serde_json::Value>(payload)?.is_object()
    {
        return Err(invalid("invalid_payload"));
    }
    Ok(())
}
fn invalid(code: &str) -> StorageError {
    StorageError::InvalidMobileCommand(code.into())
}

pub(crate) fn migrate(conn: &rusqlite::Connection) -> StorageResult<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS workspace_chat_operations(device_id TEXT NOT NULL,operation_id TEXT NOT NULL,mac_id TEXT NOT NULL,workspace_id TEXT NOT NULL,payload TEXT NOT NULL,state TEXT NOT NULL,result TEXT,PRIMARY KEY(device_id,operation_id));
        CREATE TABLE IF NOT EXISTS workspace_chat_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,device_id TEXT NOT NULL,operation_id TEXT NOT NULL,mac_id TEXT NOT NULL,workspace_id TEXT NOT NULL,kind TEXT NOT NULL,payload TEXT NOT NULL,created_at_unix_ms INTEGER NOT NULL DEFAULT 0,FOREIGN KEY(device_id,operation_id) REFERENCES workspace_chat_operations(device_id,operation_id));
        CREATE INDEX IF NOT EXISTS workspace_chat_scope ON workspace_chat_events(mac_id,workspace_id,sequence);")?;
    let current: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('workspace_chat_events') WHERE name='created_at_unix_ms')", [], |r|r.get(0))?;
    if current {
        return Ok(());
    }
    // Only legacy upgrades need a writer. Recheck under the lock when another opener got there first.
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    crate::migrations::ensure_column(
        &tx,
        "workspace_chat_events",
        "created_at_unix_ms",
        "created_at_unix_ms INTEGER NOT NULL DEFAULT 0",
    )?;
    tx.commit()?;
    Ok(())
}
