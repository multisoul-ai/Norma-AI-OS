//! AppDB atomic reception and journal. Execute all IO on a background executor.
mod acceptance;
mod events;
mod query;
mod schema;
mod types;
pub(crate) use schema::migrate;
pub use types::*;

use crate::{migrations::open_app_connection, StorageError, StorageResult};
use agent::card_instance::{
    CardInstanceEvent, CardInstanceFailure, CardInstanceReceipt, CardInstanceStage,
};
use agent::workspace_operation::{WorkspaceOperationOrigin, WorkspaceOperationSource};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub struct WorkspaceOperationStore {
    path: PathBuf,
}
impl WorkspaceOperationStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    fn write_connection(&self) -> StorageResult<Connection> {
        let connection = open_app_connection(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        Ok(connection)
    }
    fn read_connection(&self) -> StorageResult<Option<Connection>> {
        if !self.path.try_exists()? {
            return Ok(None);
        }
        let connection = Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let present: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workspace_operations')", [], |row| row.get(0))?;
        Ok(present.then_some(connection))
    }
}

pub fn workspace_operation_fingerprint(
    source: &WorkspaceOperationSource,
    items: &[WorkspaceOperationAssignment],
) -> StorageResult<String> {
    // Execution/display snapshots come from preparation, not from the user request. Repeated
    // source operations must return the original even after a Runtime/name has been replaced.
    let stable: Vec<_> = items
        .iter()
        .map(|item| {
            serde_json::json!({
                "card":item.assignment.target.card,"kind":item.kind,"text":item.assignment.text,
                "task_ref":item.task_ref,"logical_task_ref":item.logical_task_ref,"attempt_sequence":item.attempt_sequence,"force_untracked":item.assignment.force_untracked,
                "project_id":item.project_id
            })
        })
        .collect();
    digest(&serde_json::to_vec(&(source, stable))?)
}
fn digest(bytes: &[u8]) -> StorageResult<String> {
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn now_ms() -> StorageResult<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .min(i64::MAX as u128) as u64)
}
fn invalid(code: &str) -> StorageError {
    StorageError::InvalidWorkspaceOperation(code.into())
}
fn validate_original_fingerprint(value: &str) -> StorageResult<()> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("invalid_original_request_fingerprint"));
    }
    Ok(())
}
fn phase_name(phase: WorkspaceInputPhase) -> &'static str {
    match phase {
        WorkspaceInputPhase::Queued => "queued",
        WorkspaceInputPhase::Armed => "armed",
        WorkspaceInputPhase::BodyWritten => "body_written",
        WorkspaceInputPhase::EnterConfirmed => "enter_confirmed",
    }
}
fn decode_phase(value: &str) -> StorageResult<WorkspaceInputPhase> {
    match value {
        "queued" => Ok(WorkspaceInputPhase::Queued),
        "armed" => Ok(WorkspaceInputPhase::Armed),
        "body_written" => Ok(WorkspaceInputPhase::BodyWritten),
        "enter_confirmed" => Ok(WorkspaceInputPhase::EnterConfirmed),
        _ => Err(invalid("invalid_input_phase")),
    }
}
