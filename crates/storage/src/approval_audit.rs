use crate::{migrations::open_project_connection, StorageError, StorageResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io, path::PathBuf, sync::mpsc};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ApprovalAuditEntry {
    pub run_id: String,
    pub sequence: u64,
    pub node_id: u64,
    pub provider: String,
    pub request_id: String,
    pub payload: Value,
}

#[derive(Clone, Debug)]
pub struct ApprovalAuditStore {
    path: PathBuf,
    turn_store: Option<crate::MobileTurnStore>,
}

impl ApprovalAuditStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            turn_store: None,
        }
    }

    pub fn with_turn_store(mut self, store: crate::MobileTurnStore) -> Self {
        self.turn_store = Some(store);
        self
    }

    /// Startup runs off-thread; commit the exact identity before a task can begin.
    pub fn record_session(&self, session: &crate::MobileSession) -> StorageResult<()> {
        self.turn_store
            .clone()
            .unwrap_or_else(|| crate::MobileTurnStore::new(self.path.clone()))
            .record_session(session)
    }

    /// Blocking SQLite operation; use a background executor.
    pub fn append(&self, entries: &[ApprovalAuditEntry]) -> StorageResult<()> {
        append(&mut open_project_connection(&self.path)?, entries)
    }

    /// A persisted choice is an intent, never proof that the Provider executed it.
    pub fn list_run(
        &self,
        run_id: &str,
        after: u64,
        limit: usize,
    ) -> StorageResult<Vec<ApprovalAuditEntry>> {
        if run_id.is_empty() || after > i64::MAX as u64 || !(1..=256).contains(&limit) {
            return Err(invalid("invalid audit cursor or limit"));
        }
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare("SELECT entry_json FROM approval_audit WHERE run_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")?;
        let rows =
            statement.query_map(params![run_id, after, limit], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
}

enum Entry {
    Approvals(Vec<ApprovalAuditEntry>),
    Turn(crate::MobileTurnOutcome),
    Transcript(crate::MobileSessionKey, PathBuf),
}
type Write = (Entry, mpsc::SyncSender<StorageResult<()>>);

/// Bounded serial writer. Receipts are sent only after the SQLite transaction commits.
pub struct BackgroundApprovalAudit {
    sender: mpsc::SyncSender<Write>,
}

impl BackgroundApprovalAudit {
    /// Opens the database before returning: call off the UI thread and fail launch on error.
    pub fn new(store: ApprovalAuditStore) -> StorageResult<Self> {
        let mut connection = open_project_connection(&store.path)?;
        let mut turns = store
            .turn_store
            .map(|store| {
                store
                    .connection()
                    .map(|connection| (connection, store.workspace_id.unwrap_or_default()))
            })
            .transpose()?;
        // 8 queued batches * at most 1 MiB serialized entries = at most 8 MiB queued audit data.
        let (sender, receiver) = mpsc::sync_channel::<Write>(8);
        std::thread::Builder::new()
            .name("norma-approval-audit".into())
            .spawn(move || {
                while let Ok((entries, reply)) = receiver.recv() {
                    let result = match entries {
                        Entry::Approvals(entries) => append(&mut connection, &entries),
                        Entry::Turn(outcome) => match &mut turns {
                            Some((connection, workspace)) => {
                                crate::mobile_turn_store::record(connection, workspace, &outcome)
                            }
                            None => crate::mobile_turn_store::record(&mut connection, "", &outcome),
                        },
                        Entry::Transcript(key, path) => match &mut turns {
                            Some((connection, workspace)) => {
                                crate::mobile_session_store::bind_transcript(
                                    connection, workspace, &key, &path,
                                )
                            }
                            None => crate::mobile_session_store::bind_transcript(
                                &mut connection,
                                "",
                                &key,
                                &path,
                            ),
                        },
                    };
                    let _ = reply.try_send(result);
                }
            })?;
        Ok(Self { sender })
    }

    /// Only an authenticated local Hook may supply a reference. Body/path containment is
    /// independently checked by the Provider reader; mobile requests never supply paths.
    pub fn bind_session_transcript(
        &self,
        key: crate::MobileSessionKey,
        path: PathBuf,
    ) -> StorageResult<mpsc::Receiver<StorageResult<()>>> {
        crate::mobile_session_store::validate_transcript(&key, &path)?;
        let (reply, receipt) = mpsc::sync_channel(1);
        self.sender
            .try_send((Entry::Transcript(key, path), reply))
            .map_err(|_| invalid("session reference writer full or closed"))?;
        Ok(receipt)
    }

    /// Reuse this run's bounded serial writer; terminal evidence has its own immutable table.
    pub fn record_turn(
        &self,
        outcome: crate::MobileTurnOutcome,
    ) -> StorageResult<mpsc::Receiver<StorageResult<()>>> {
        outcome.key.validate()?;
        let (reply, receipt) = mpsc::sync_channel(1);
        self.sender
            .try_send((Entry::Turn(outcome), reply))
            .map_err(|_| invalid("turn outcome writer full or closed"))?;
        Ok(receipt)
    }

    /// Enqueue only; losing the receipt never retries a Provider command.
    pub fn append(
        &self,
        entries: Vec<ApprovalAuditEntry>,
    ) -> StorageResult<mpsc::Receiver<StorageResult<()>>> {
        validate(&entries)?;
        let (reply, receipt) = mpsc::sync_channel(1);
        self.sender
            .try_send((Entry::Approvals(entries), reply))
            .map_err(|_| invalid("approval audit writer full or closed"))?;
        Ok(receipt)
    }
}

fn validate(entries: &[ApprovalAuditEntry]) -> StorageResult<()> {
    if entries.is_empty() || entries.len() > 256 {
        return Err(invalid("invalid audit batch size"));
    }
    let mut size = 0;
    for entry in entries {
        if entry.run_id.is_empty()
            || entry.run_id.len() > 256
            || entry.request_id.is_empty()
            || entry.request_id.len() > 512
            || entry.sequence == 0
            || entry.sequence > i64::MAX as u64
            || entry.node_id > i64::MAX as u64
            || !matches!(entry.provider.as_str(), "codex" | "claude")
        {
            return Err(invalid("invalid approval audit identity"));
        }
        size += serde_json::to_vec(entry)?.len();
        if size > 1024 * 1024 {
            return Err(invalid("approval audit batch exceeds 1 MiB"));
        }
    }
    Ok(())
}

fn append(connection: &mut Connection, entries: &[ApprovalAuditEntry]) -> StorageResult<()> {
    validate(entries)?;
    let transaction = connection.transaction()?;
    for entry in entries {
        let encoded = serde_json::to_string(entry)?;
        let inserted = transaction.execute(
            "INSERT INTO approval_audit(run_id,sequence,entry_json,recorded_at_unix) VALUES (?1,?2,?3,?4) ON CONFLICT(run_id,sequence) DO NOTHING",
            params![entry.run_id,entry.sequence,encoded,crate::current_unix_timestamp()?],
        )?;
        if inserted == 0 {
            let original: String = transaction.query_row(
                "SELECT entry_json FROM approval_audit WHERE run_id=?1 AND sequence=?2",
                params![entry.run_id, entry.sequence],
                |row| row.get(0),
            )?;
            let original: ApprovalAuditEntry = serde_json::from_str(&original)?;
            if original != *entry {
                return Err(invalid("conflicting approval audit identity"));
            }
        }
    }
    transaction.commit()?;
    Ok(())
}

fn invalid(message: &str) -> StorageError {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}
