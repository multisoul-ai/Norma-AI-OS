use crate::{
    current_unix_timestamp, migrations::open_project_connection, StorageError, StorageResult,
};
use canvas::{AgentProfile, NodeId};
use rusqlite::params;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionCheckpointKind {
    Terminal,
    Agent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionCheckpoint {
    pub node_id: NodeId,
    pub kind: SessionCheckpointKind,
    pub payload: String,
    pub cols: u16,
    pub rows: u16,
    pub shell: Option<String>,
    pub agent_profile: Option<AgentProfile>,
}

#[derive(Clone, Debug)]
pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn new(path: PathBuf) -> Self {
        SessionStore { path }
    }

    pub fn save_checkpoint(&self, checkpoint: &SessionCheckpoint) -> StorageResult<()> {
        let connection = open_project_connection(&self.path)?;
        connection.execute(
            r#"
            INSERT INTO session_checkpoints (
                node_id, kind, payload, cols, rows, shell, agent_profile, updated_at_unix
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(node_id) DO UPDATE SET
                kind = excluded.kind,
                payload = excluded.payload,
                cols = excluded.cols,
                rows = excluded.rows,
                shell = excluded.shell,
                agent_profile = excluded.agent_profile,
                updated_at_unix = excluded.updated_at_unix
            "#,
            params![
                checkpoint.node_id,
                encode_kind(checkpoint.kind),
                checkpoint.payload,
                checkpoint.cols,
                checkpoint.rows,
                checkpoint.shell,
                encode_agent_profile(checkpoint.agent_profile),
                current_unix_timestamp()?,
            ],
        )?;
        Ok(())
    }

    pub fn load_checkpoints(&self) -> StorageResult<Vec<SessionCheckpoint>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            r#"
            SELECT node_id, kind, payload, cols, rows, shell, agent_profile
            FROM session_checkpoints
            ORDER BY node_id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, NodeId>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?;

        let mut checkpoints = Vec::new();
        for row in rows {
            let (node_id, kind, payload, cols, rows, shell, agent_profile) = row?;
            checkpoints.push(SessionCheckpoint {
                node_id,
                kind: decode_kind(&kind)?,
                payload,
                cols: decode_size("cols", cols)?,
                rows: decode_size("rows", rows)?,
                shell,
                agent_profile: decode_agent_profile(agent_profile)?,
            });
        }
        Ok(checkpoints)
    }
}

fn encode_kind(kind: SessionCheckpointKind) -> &'static str {
    match kind {
        SessionCheckpointKind::Terminal => "terminal",
        SessionCheckpointKind::Agent => "agent",
    }
}

fn decode_kind(kind: &str) -> StorageResult<SessionCheckpointKind> {
    match kind {
        "terminal" => Ok(SessionCheckpointKind::Terminal),
        "agent" => Ok(SessionCheckpointKind::Agent),
        other => Err(StorageError::InvalidSessionKind(other.to_string())),
    }
}

fn encode_agent_profile(profile: Option<AgentProfile>) -> Option<&'static str> {
    profile.map(|profile| match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
    })
}

fn decode_agent_profile(profile: Option<String>) -> StorageResult<Option<AgentProfile>> {
    let Some(profile) = profile else {
        return Ok(None);
    };
    match profile.as_str() {
        "claude" => Ok(Some(AgentProfile::Claude)),
        "codex" => Ok(Some(AgentProfile::Codex)),
        other => Err(StorageError::InvalidAgentProfile(other.to_string())),
    }
}

fn decode_size(field: &'static str, value: i64) -> StorageResult<u16> {
    u16::try_from(value).map_err(|_| StorageError::InvalidSessionSize { field, value })
}

#[cfg(test)]
#[path = "session_store_tests.rs"]
mod tests;
