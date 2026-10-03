use crate::{migrations::open_app_connection, StorageResult};
use agent::{AdapterError, MemorySourceReference, MemoryToolOutput};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemorySourceDetail {
    pub reference: MemorySourceReference,
    pub text: String,
    pub recorded_at: u64,
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_scope_bindings (
        identity TEXT PRIMARY KEY NOT NULL, memory_id TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS memory_sources (
        reference TEXT PRIMARY KEY NOT NULL, text TEXT NOT NULL, recorded_at INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS memory_operations (
        operation_id TEXT PRIMARY KEY NOT NULL, fingerprint TEXT NOT NULL,
        receipt TEXT, recorded_at INTEGER NOT NULL, record_json TEXT NOT NULL);",
    )?;
    crate::migrations::ensure_column(
        connection,
        "memory_operations",
        "record_json",
        "record_json TEXT NOT NULL DEFAULT '{}'",
    )?;
    Ok(())
}

pub(super) fn failure(error: impl std::fmt::Display) -> AdapterError {
    AdapterError::Failed(error.to_string())
}

pub(super) fn source(
    path: &Path,
    reference: &MemorySourceReference,
    text: &str,
) -> Result<(), AdapterError> {
    let mut database = open_app_connection(path).map_err(failure)?;
    let connection = database
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(failure)?;
    let key = serde_json::to_string(reference).map_err(failure)?;
    let old: Option<String> = connection
        .query_row(
            "SELECT text FROM memory_sources WHERE reference=?1",
            [&key],
            |row| row.get(0),
        )
        .optional()
        .map_err(failure)?;
    if old.as_deref().is_some_and(|old| old != text) {
        return Err(AdapterError::Rejected("来源身份重复且原话不同".into()));
    }
    connection
        .execute(
            "INSERT OR IGNORE INTO memory_sources VALUES (?1,?2,?3)",
            params![key, text, crate::current_unix_timestamp().map_err(failure)?],
        )
        .map_err(failure)?;
    connection.commit().map_err(failure)?;
    Ok(())
}

pub(super) fn details(
    path: &Path,
    references: &[MemorySourceReference],
) -> Result<Vec<MemorySourceDetail>, AdapterError> {
    let connection = open_app_connection(path).map_err(failure)?;
    references
        .iter()
        .map(|reference| {
            let key = serde_json::to_string(reference).map_err(failure)?;
            connection
                .query_row(
                    "SELECT text,recorded_at FROM memory_sources WHERE reference=?1",
                    [key],
                    |row| {
                        Ok(MemorySourceDetail {
                            reference: reference.clone(),
                            text: row.get(0)?,
                            recorded_at: row.get(1)?,
                        })
                    },
                )
                .map_err(failure)
        })
        .collect()
}

/// Reserve before publishing. An unresolved reservation is never replayed.
pub(super) fn reserve(
    path: &Path,
    operation: &str,
    fingerprint: &str,
    record: &str,
) -> Result<Option<Result<MemoryToolOutput, agent::MemoryRevision>>, AdapterError> {
    let connection = open_app_connection(path).map_err(failure)?;
    let inserted = connection
        .execute(
            "INSERT OR IGNORE INTO memory_operations (operation_id,fingerprint,receipt,recorded_at,record_json) VALUES (?1,?2,NULL,?3,?4)",
            params![
                operation,
                fingerprint,
                crate::current_unix_timestamp().map_err(failure)?,record
            ],
        )
        .map_err(failure)?;
    if inserted == 1 {
        return Ok(None);
    }
    let (old, receipt, record): (String, Option<String>, String) = connection
        .query_row(
            "SELECT fingerprint,receipt,record_json FROM memory_operations WHERE operation_id=?1",
            [operation],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(failure)?;
    if old != fingerprint {
        return Err(AdapterError::Rejected("操作身份重复且候选内容不同".into()));
    }
    if let Some(receipt) = receipt {
        return Ok(Some(Ok(serde_json::from_str(&receipt).map_err(failure)?)));
    }
    let record: serde_json::Value = serde_json::from_str(&record).map_err(failure)?;
    let expected = serde_json::from_value(record[1].clone())
        .map_err(|error| AdapterError::Unknown(format!("无法确认原保存基线：{error}")))?;
    Ok(Some(Err(expected)))
}

pub(super) fn finish(
    path: &Path,
    operation: &str,
    output: &MemoryToolOutput,
) -> Result<(), AdapterError> {
    let connection = open_app_connection(path).map_err(failure)?;
    connection
        .execute(
            "UPDATE memory_operations SET receipt=?2 WHERE operation_id=?1",
            params![operation, serde_json::to_string(output).map_err(failure)?],
        )
        .map_err(failure)?;
    Ok(())
}
