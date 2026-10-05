use crate::{migrations::open_project_connection, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MobileTurnKey {
    pub node_id: u64,
    pub profile: String,
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
}

impl MobileTurnKey {
    pub fn validate(&self) -> StorageResult<()> {
        if self.node_id == 0
            || self.node_id > i64::MAX as u64
            || !crate::is_mobile_agent_profile(&self.profile)
            || [&self.run_id, &self.session_id, &self.turn_id]
                .iter()
                .any(|s| {
                    s.is_empty()
                        || s.len() > 128
                        || !s
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                })
        {
            return Err(invalid("invalid mobile turn identity"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MobileTurnState {
    Completed,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MobileTurnOutcome {
    pub key: MobileTurnKey,
    pub state: MobileTurnState,
    /// Final reply excerpt frozen with this turn; old outcomes decode without a preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RecordedMobileTurn {
    pub sequence: u64,
    pub recorded_at_unix: u64,
    pub outcome: MobileTurnOutcome,
}

#[derive(Clone, Debug)]
pub struct MobileTurnStore {
    pub(crate) path: PathBuf,
    pub(crate) workspace_id: Option<String>,
}
impl MobileTurnStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            workspace_id: None,
        }
    }
    /// Shared AppDB storage is explicitly scoped to the locally bound workspace.
    pub fn for_workspace(path: PathBuf, workspace_id: String) -> Self {
        Self {
            path,
            workspace_id: Some(workspace_id),
        }
    }
    pub(crate) fn connection(&self) -> StorageResult<Connection> {
        if let Some(id) = &self.workspace_id {
            validate_workspace_scope(id)?;
            crate::migrations::open_app_connection(&self.path)
        } else {
            open_project_connection(&self.path)
        }
    }
    /// Blocking project database read. No event means unknown, never successful completion.
    pub fn get(&self, key: &MobileTurnKey) -> StorageResult<Option<MobileTurnOutcome>> {
        key.validate()?;
        let connection = self.connection()?;
        let row = read(
            &connection,
            self.workspace_id.as_deref().unwrap_or(""),
            &key.run_id,
            &key.turn_id,
        )?;
        Ok(row.filter(|row| row.key == *key))
    }

    /// Start a live consumer at committed history; starting notifications never replays old tasks.
    pub fn latest_sequence(&self) -> StorageResult<u64> {
        Ok(self.connection()?.query_row(
            "SELECT coalesce(max(rowid),0) FROM mobile_turn_outcomes WHERE workspace_id=?1",
            [self.workspace_id.as_deref().unwrap_or("")],
            |row| row.get(0),
        )?)
    }

    /// Committed events only, scoped exactly like get; rowid orders same-second completions.
    pub fn after(&self, sequence: u64, limit: usize) -> StorageResult<Vec<RecordedMobileTurn>> {
        if sequence > i64::MAX as u64 || !(1..=256).contains(&limit) {
            return Err(invalid("invalid turn event cursor"));
        }
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT rowid,recorded_at_unix,outcome_json FROM mobile_turn_outcomes
            WHERE workspace_id=?1 AND rowid>?2 ORDER BY rowid LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![self.workspace_id.as_deref().unwrap_or(""), sequence, limit],
            |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, String>(2)?)),
        )?;
        rows.map(|row| {
            let (sequence, recorded_at_unix, json) = row?;
            let outcome: MobileTurnOutcome = serde_json::from_str(&json)?;
            outcome.key.validate()?;
            Ok(RecordedMobileTurn {
                sequence,
                recorded_at_unix,
                outcome,
            })
        })
        .collect()
    }
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS mobile_turn_outcomes (
        workspace_id TEXT NOT NULL, run_id TEXT NOT NULL, turn_id TEXT NOT NULL, outcome_json TEXT NOT NULL,
        recorded_at_unix INTEGER NOT NULL, PRIMARY KEY(workspace_id,run_id,turn_id)
    )",
    )?;
    Ok(())
}

pub(crate) fn read(
    connection: &Connection,
    workspace: &str,
    run: &str,
    turn: &str,
) -> StorageResult<Option<MobileTurnOutcome>> {
    let raw: Option<String> = connection
        .query_row(
            "SELECT outcome_json FROM mobile_turn_outcomes WHERE workspace_id=?1 AND run_id=?2 AND turn_id=?3",
            params![workspace, run, turn],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|raw| Ok(serde_json::from_str(&raw)?)).transpose()
}

pub(crate) fn record(
    connection: &mut Connection,
    workspace: &str,
    outcome: &MobileTurnOutcome,
) -> StorageResult<()> {
    outcome.key.validate()?;
    let transaction = connection.transaction()?;
    let inserted = transaction.execute(
        "INSERT INTO mobile_turn_outcomes(workspace_id,run_id,turn_id,outcome_json,recorded_at_unix) VALUES(?1,?2,?3,?4,?5)
        ON CONFLICT(workspace_id,run_id,turn_id) DO NOTHING",
        params![workspace,outcome.key.run_id,outcome.key.turn_id,serde_json::to_string(outcome)?,crate::current_unix_timestamp()?],
    )?;
    if inserted == 0
        && read(
            &transaction,
            workspace,
            &outcome.key.run_id,
            &outcome.key.turn_id,
        )?
        .is_none_or(|saved| saved.key != outcome.key || saved.state != outcome.state)
    {
        return Err(invalid("conflicting mobile turn outcome"));
    }
    transaction.commit()?;
    Ok(())
}
fn invalid(message: &str) -> StorageError {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}

/// Runtime scope is distinct from the flat project IDs used in device grants.
pub(crate) fn validate_workspace_scope(scope: &str) -> StorageResult<()> {
    if let Some(rest) = scope.strip_prefix("desktop:") {
        let (workspace, area) = rest
            .split_once(':')
            .ok_or_else(|| invalid("invalid desktop scope"))?;
        crate::mobile_access_store::validate_id(workspace)?;
        crate::mobile_access_store::validate_id(area)
    } else {
        crate::mobile_access_store::validate_id(scope)
    }
}
