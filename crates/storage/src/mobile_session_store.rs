use crate::{MobileTurnStore, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MobileSessionKey {
    pub node_id: u64,
    pub profile: String,
    pub run_id: String,
    pub session_id: String,
}

impl MobileSessionKey {
    pub fn validate(&self) -> StorageResult<()> {
        if self.node_id == 0
            || self.node_id > i64::MAX as u64
            || !crate::is_mobile_agent_profile(&self.profile)
            || [&self.run_id, &self.session_id].into_iter().any(|s| {
                s.is_empty()
                    || s.len() > 128
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            })
        {
            return Err(invalid("invalid mobile session identity"));
        }
        Ok(())
    }
}

/// Immutable launch identity. Provider files remain the only source of full conversation text.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MobileSession {
    pub key: MobileSessionKey,
    pub workspace_root: PathBuf,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct RecordedMobileSession {
    pub sequence: u64,
    pub recorded_at_unix: u64,
    pub session: MobileSession,
    pub transcript_path: Option<PathBuf>,
}

// Session references use the same explicitly bound database/workspace as turn evidence.
impl MobileTurnStore {
    /// Background preparation must commit identity before sending the first model task.
    pub fn record_session(&self, session: &MobileSession) -> StorageResult<()> {
        session.key.validate()?;
        valid_path(&session.workspace_root)?;
        if session.title.is_empty() || session.title.len() > 4096 {
            return Err(invalid("invalid session title"));
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let workspace = self.workspace_id.as_deref().unwrap_or("");
        let inserted = transaction.execute(
            "INSERT INTO mobile_sessions(workspace_id,run_id,session_json,recorded_at_unix)
             VALUES(?1,?2,?3,?4) ON CONFLICT(workspace_id,run_id) DO NOTHING",
            params![
                workspace,
                session.key.run_id,
                serde_json::to_string(session)?,
                crate::current_unix_timestamp()?
            ],
        )?;
        if inserted == 0
            && read(&transaction, workspace, &session.key.run_id)?
                .map(|r| r.session)
                .as_ref()
                != Some(session)
        {
            return Err(invalid("conflicting mobile session identity"));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn session(&self, key: &MobileSessionKey) -> StorageResult<Option<RecordedMobileSession>> {
        key.validate()?;
        Ok(read(
            &self.connection()?,
            self.workspace_id.as_deref().unwrap_or(""),
            &key.run_id,
        )?
        .filter(|row| row.session.key == *key))
    }

    /// Indexed keyset pagination: 20 metadata records, no provider scanning or body copies.
    pub fn sessions(&self, before: Option<u64>) -> StorageResult<Vec<RecordedMobileSession>> {
        let before = before.unwrap_or(i64::MAX as u64);
        if before == 0 || before > i64::MAX as u64 {
            return Err(invalid("invalid session cursor"));
        }
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT sequence,recorded_at_unix,session_json,transcript_path FROM mobile_sessions
             WHERE workspace_id=?1 AND sequence<?2 ORDER BY sequence DESC LIMIT 20",
        )?;
        let rows = statement.query_map(
            params![self.workspace_id.as_deref().unwrap_or(""), before],
            row,
        )?;
        rows.map(|row| decode(row?)).collect()
    }
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_sessions (
            sequence INTEGER PRIMARY KEY, workspace_id TEXT NOT NULL, run_id TEXT NOT NULL,
            session_json TEXT NOT NULL, recorded_at_unix INTEGER NOT NULL, transcript_path TEXT,
            UNIQUE(workspace_id,run_id)
        );
        CREATE INDEX IF NOT EXISTS idx_mobile_sessions_workspace ON mobile_sessions(workspace_id,sequence);",
    )?;
    Ok(())
}

type Row = (u64, u64, String, Option<String>);
fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}
fn decode((sequence, recorded_at_unix, json, path): Row) -> StorageResult<RecordedMobileSession> {
    let session: MobileSession = serde_json::from_str(&json)?;
    session.key.validate()?;
    Ok(RecordedMobileSession {
        sequence,
        recorded_at_unix,
        session,
        transcript_path: path.map(PathBuf::from),
    })
}
fn read(
    connection: &Connection,
    workspace: &str,
    run: &str,
) -> StorageResult<Option<RecordedMobileSession>> {
    connection.query_row(
        "SELECT sequence,recorded_at_unix,session_json,transcript_path FROM mobile_sessions WHERE workspace_id=?1 AND run_id=?2",
        params![workspace,run], row,
    ).optional()?.map(decode).transpose()
}

pub(crate) fn validate_transcript(key: &MobileSessionKey, path: &Path) -> StorageResult<()> {
    key.validate()?;
    valid_path(path)?;
    if !matches!(key.profile.as_str(), "Claude" | "Cursor")
        || path.file_name().and_then(|s| s.to_str()) != Some(&format!("{}.jsonl", key.session_id))
    {
        return Err(invalid("transcript does not match native session"));
    }
    Ok(())
}

pub(crate) fn bind_transcript(
    connection: &mut Connection,
    workspace: &str,
    key: &MobileSessionKey,
    path: &Path,
) -> StorageResult<()> {
    validate_transcript(key, path)?;
    let transaction = connection.transaction()?;
    let original = read(&transaction, workspace, &key.run_id)?
        .filter(|r| r.session.key == *key)
        .ok_or_else(|| invalid("mobile session not registered"))?;
    if original.transcript_path.as_ref().is_some_and(|p| p != path) {
        return Err(invalid("conflicting transcript reference"));
    }
    transaction.execute(
        "UPDATE mobile_sessions SET transcript_path=?1 WHERE workspace_id=?2 AND run_id=?3",
        params![
            path.to_str()
                .ok_or_else(|| invalid("transcript path is not UTF-8"))?,
            workspace,
            key.run_id
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

fn valid_path(path: &Path) -> StorageResult<()> {
    if !path.is_absolute()
        || path.as_os_str().len() > 4096
        || path.components().any(|c| matches!(c, Component::ParentDir))
        || path.to_str().is_none_or(|s| s.contains('\0'))
    {
        return Err(invalid("invalid session filesystem reference"));
    }
    Ok(())
}
fn invalid(message: &str) -> StorageError {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}

impl MobileTurnStore {
    /// Persist an authenticated native Hook off-thread, reusing the existing immutable session/turn tables.
    pub fn record_native_event(
        &self,
        session: &MobileSession,
        transcript: Option<&Path>,
        outcome: Option<&crate::MobileTurnOutcome>,
    ) -> StorageResult<()> {
        if outcome.is_some_and(|value| {
            value.key.node_id != session.key.node_id
                || value.key.profile != session.key.profile
                || value.key.run_id != session.key.run_id
                || value.key.session_id != session.key.session_id
        }) {
            return Err(invalid("native event session mismatch"));
        }
        match self.session(&session.key)? {
            Some(saved) if saved.session.workspace_root != session.workspace_root => {
                return Err(invalid("native event workspace mismatch"))
            }
            Some(_) => {}
            None => self.record_session(session)?,
        }
        let mut connection = self.connection()?;
        let workspace = self.workspace_id.as_deref().unwrap_or("");
        if let Some(path) = transcript {
            bind_transcript(&mut connection, workspace, &session.key, path)?;
        }
        if let Some(outcome) = outcome {
            crate::mobile_turn_store::record(&mut connection, workspace, outcome)?;
        }
        Ok(())
    }
}
