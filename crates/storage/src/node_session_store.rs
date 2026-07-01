use crate::{StorageError, StorageResult};
use canvas::NodeId;
use rusqlite::{params, Connection};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeSession {
    pub node_id: NodeId,
    pub kind: NodeSessionKind,
    pub cwd: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeSessionKind {
    Terminal,
    Agent,
}

impl NodeSessionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Agent => "agent",
        }
    }

    fn from_str(s: &str) -> StorageResult<Self> {
        match s {
            "terminal" => Ok(Self::Terminal),
            "agent" => Ok(Self::Agent),
            other => Err(StorageError::InvalidNodeSessionKind(other.to_string())),
        }
    }
}

pub struct NodeSessionStore {
    path: PathBuf,
}

impl NodeSessionStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn save_session(&self, session: &NodeSession) -> StorageResult<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute(
            "INSERT OR REPLACE INTO node_sessions \
             (node_id, kind, cwd, session_id, updated_at_unix) \
             VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))",
            params![
                session.node_id,
                session.kind.as_str(),
                session.cwd,
                session.session_id,
            ],
        )?;
        Ok(())
    }

    pub fn replace_all_sessions(&self, sessions: &[NodeSession]) -> StorageResult<()> {
        let mut conn = Connection::open(&self.path)?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM node_sessions", [])?;
        {
            let mut statement = tx.prepare(
                "INSERT INTO node_sessions \
                 (node_id, kind, cwd, session_id, updated_at_unix) \
                 VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))",
            )?;
            for session in sessions {
                statement.execute(params![
                    session.node_id,
                    session.kind.as_str(),
                    session.cwd,
                    session.session_id,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_sessions(&self) -> StorageResult<Vec<NodeSession>> {
        let conn = Connection::open(&self.path)?;
        let mut stmt = conn.prepare("SELECT node_id, kind, cwd, session_id FROM node_sessions")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?
            .map(|r| {
                let (node_id, kind_str, cwd, session_id) = r?;
                let kind = NodeSessionKind::from_str(&kind_str)?;
                Ok(NodeSession {
                    node_id,
                    kind,
                    cwd,
                    session_id,
                })
            })
            .collect::<StorageResult<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
#[path = "../tests/unit/node_session_store_tests.rs"]
mod tests;
