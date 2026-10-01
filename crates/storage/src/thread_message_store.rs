use crate::{current_unix_timestamp, migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct NewThreadMessage {
    pub device_id: String,
    pub source_mac_id: String,
    pub collaboration_workspace_id: String,
    pub thread_id: String,
    pub operation_id: String,
    pub message_id: String,
    pub origin_device_id: String,
    pub fingerprint: String,
    /// Canonical protocol JSON, validated by the domain protocol before reaching storage.
    pub payload: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ThreadReceipt {
    pub source_mac_id: String,
    pub collaboration_workspace_id: String,
    pub thread_id: String,
    pub operation_id: String,
    pub message_id: String,
    pub origin_device_id: String,
    pub fingerprint: String,
    pub received_at_unix: u64,
    pub event_id: String,
    pub cursor: u64,
    pub state: String,
}

pub struct StoredThreadMessage {
    pub receipt: ThreadReceipt,
    pub payload: String,
}

#[derive(Clone)]
pub struct ThreadMessageStore {
    path: PathBuf,
}

impl ThreadMessageStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn receive(&self, message: &NewThreadMessage) -> StorageResult<ThreadReceipt> {
        validate(message)?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = read(&tx, &message.device_id, &message.operation_id)? {
            let r = &existing.receipt;
            if r.source_mac_id != message.source_mac_id
                || r.collaboration_workspace_id != message.collaboration_workspace_id
                || r.thread_id != message.thread_id
                || r.message_id != message.message_id
                || r.origin_device_id != message.origin_device_id
                || r.fingerprint != message.fingerprint
                || existing.payload != message.payload
            {
                return Err(invalid("thread operation conflict"));
            }
            tx.commit()?;
            return Ok(existing.receipt);
        }
        let other: Option<String> = tx.query_row(
            "SELECT operation_id FROM mobile_thread_messages WHERE device_id=?1 AND message_id=?2",
            params![message.device_id, message.message_id], |r| r.get(0)).optional()?;
        if other.is_some() {
            return Err(invalid("message operation conflict"));
        }
        tx.execute("INSERT OR IGNORE INTO mobile_collaboration_threads(device_id,thread_id,collaboration_workspace_id,source_mac_id,origin_device_id) VALUES(?1,?2,?3,?4,?5)",
            params![message.device_id, message.thread_id, message.collaboration_workspace_id, message.source_mac_id, message.origin_device_id])?;
        let binding: (String, String, String) = tx.query_row(
            "SELECT collaboration_workspace_id,source_mac_id,origin_device_id FROM mobile_collaboration_threads WHERE device_id=?1 AND thread_id=?2",
            params![message.device_id, message.thread_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        if binding
            != (
                message.collaboration_workspace_id.clone(),
                message.source_mac_id.clone(),
                message.origin_device_id.clone(),
            )
        {
            return Err(invalid("thread binding conflict"));
        }
        tx.execute("INSERT INTO mobile_thread_messages(device_id,operation_id,thread_id,message_id,fingerprint,payload_json,received_at_unix) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![message.device_id, message.operation_id, message.thread_id, message.message_id,
                message.fingerprint, message.payload, current_unix_timestamp()?])?;
        tx.execute("INSERT INTO mobile_thread_events(device_id,operation_id,kind) VALUES(?1,?2,'received')",
            params![message.device_id, message.operation_id])?;
        let receipt = read(&tx, &message.device_id, &message.operation_id)?
            .ok_or_else(|| invalid("receipt missing"))?
            .receipt;
        tx.commit()?;
        Ok(receipt)
    }

    pub fn get(
        &self,
        device: &str,
        mac: &str,
        workspace: &str,
        thread: &str,
        operation: &str,
    ) -> StorageResult<Option<StoredThreadMessage>> {
        for id in [device, mac, workspace, thread, operation] {
            crate::mobile_access_store::validate_id(id)?;
        }
        let connection = open_app_connection(&self.path)?;
        Ok(read(&connection, device, operation)?
            .filter(|m| scope(&m.receipt, mac, workspace, thread)))
    }

    /// Cursors are durable events, never an in-memory socket counter. Gaps belong to other threads.
    pub fn events(
        &self,
        device: &str,
        mac: &str,
        workspace: &str,
        thread: &str,
        after: u64,
    ) -> StorageResult<Vec<StoredThreadMessage>> {
        for id in [device, mac, workspace, thread] {
            crate::mobile_access_store::validate_id(id)?;
        }
        let after = i64::try_from(after).map_err(|_| invalid("invalid event cursor"))?;
        let connection = open_app_connection(&self.path)?;
        let mut query = connection.prepare(&(SELECT.to_owned() + " WHERE m.device_id=?1 AND t.source_mac_id=?2 AND t.collaboration_workspace_id=?3 AND m.thread_id=?4 AND e.sequence>?5 ORDER BY e.sequence LIMIT 32"))?;
        let rows = query.query_map(params![device, mac, workspace, thread, after], row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }
}

const SELECT: &str = "SELECT t.source_mac_id,t.collaboration_workspace_id,m.thread_id,m.operation_id,m.message_id,t.origin_device_id,m.fingerprint,m.received_at_unix,e.sequence,m.payload_json FROM mobile_thread_messages m JOIN mobile_collaboration_threads t ON t.device_id=m.device_id AND t.thread_id=m.thread_id JOIN mobile_thread_events e ON e.device_id=m.device_id AND e.operation_id=m.operation_id AND e.kind='received'";

fn read(
    connection: &Connection,
    device: &str,
    operation: &str,
) -> StorageResult<Option<StoredThreadMessage>> {
    Ok(connection
        .query_row(
            &(SELECT.to_owned() + " WHERE m.device_id=?1 AND m.operation_id=?2"),
            params![device, operation],
            row,
        )
        .optional()?)
}

fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredThreadMessage> {
    let source_mac_id: String = row.get(0)?;
    let cursor: u64 = row.get(8)?;
    Ok(StoredThreadMessage {
        receipt: ThreadReceipt {
            event_id: format!("{source_mac_id}:{cursor}"),
            source_mac_id,
            collaboration_workspace_id: row.get(1)?,
            thread_id: row.get(2)?,
            operation_id: row.get(3)?,
            message_id: row.get(4)?,
            origin_device_id: row.get(5)?,
            fingerprint: row.get(6)?,
            received_at_unix: row.get(7)?,
            cursor,
            state: "received".into(),
        },
        payload: row.get(9)?,
    })
}

fn scope(receipt: &ThreadReceipt, mac: &str, workspace: &str, thread: &str) -> bool {
    receipt.source_mac_id == mac
        && receipt.collaboration_workspace_id == workspace
        && receipt.thread_id == thread
}

fn validate(message: &NewThreadMessage) -> StorageResult<()> {
    for id in [
        &message.device_id,
        &message.source_mac_id,
        &message.collaboration_workspace_id,
        &message.thread_id,
        &message.operation_id,
        &message.message_id,
        &message.origin_device_id,
    ] {
        crate::mobile_access_store::validate_id(id)?;
    }
    if message.fingerprint.len() != 64
        || !message.fingerprint.bytes().all(|v| v.is_ascii_hexdigit())
        || message.payload.len() > 192 * 1024
        || !serde_json::from_str::<serde_json::Value>(&message.payload)?.is_object()
    {
        return Err(invalid("invalid thread payload"));
    }
    Ok(())
}

fn invalid(message: &str) -> StorageError {
    StorageError::InvalidMobileCommand(message.into())
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS mobile_collaboration_threads(
        device_id TEXT NOT NULL,thread_id TEXT NOT NULL,collaboration_workspace_id TEXT NOT NULL,
        source_mac_id TEXT NOT NULL,origin_device_id TEXT NOT NULL,PRIMARY KEY(device_id,thread_id));
        CREATE TABLE IF NOT EXISTS mobile_thread_messages(
        device_id TEXT NOT NULL,operation_id TEXT NOT NULL,thread_id TEXT NOT NULL,message_id TEXT NOT NULL,
        fingerprint TEXT NOT NULL,payload_json TEXT NOT NULL,received_at_unix INTEGER NOT NULL,
        PRIMARY KEY(device_id,operation_id),UNIQUE(device_id,message_id),
        FOREIGN KEY(device_id,thread_id) REFERENCES mobile_collaboration_threads(device_id,thread_id));
        CREATE TABLE IF NOT EXISTS mobile_thread_events(
        sequence INTEGER PRIMARY KEY AUTOINCREMENT,device_id TEXT NOT NULL,operation_id TEXT NOT NULL,
        kind TEXT NOT NULL CHECK(kind IN ('received')),UNIQUE(device_id,operation_id,kind),
        FOREIGN KEY(device_id,operation_id) REFERENCES mobile_thread_messages(device_id,operation_id));
        CREATE INDEX IF NOT EXISTS mobile_thread_scope ON mobile_thread_messages(device_id,thread_id);")?;
    Ok(())
}
