use crate::{migrations::open_app_connection, StorageResult};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesktopAgentStatus {
    pub node_id: u64,
    pub name: String,
    pub task: Option<String>,
    pub state: String,
    pub run_id: Option<String>,
    pub turn_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopAreaStatus {
    pub area_id: String,
    pub updated_at_unix_ms: i64,
    pub agents: Vec<DesktopAgentStatus>,
}

#[derive(Clone, Debug)]
pub struct DesktopAreaStatusStore {
    path: PathBuf,
}

impl DesktopAreaStatusStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Ignore an older asynchronous write and never create a cache for an unrelated area.
    pub fn save(
        &self,
        workspace_id: &str,
        area_id: &str,
        updated_at_unix_ms: i64,
        agents: &[DesktopAgentStatus],
    ) -> StorageResult<bool> {
        let connection = open_app_connection(&self.path)?;
        let belongs_to_workspace: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM desktop_project_areas WHERE desktop_workspace_id=?1 AND id=?2)",
            params![workspace_id, area_id],
            |row| row.get(0),
        )?;
        if !belongs_to_workspace {
            return Ok(false);
        }
        let agents_json = serde_json::to_string(agents)?;
        let changed = connection.execute(
            "INSERT INTO desktop_area_status(workspace_id,area_id,updated_at_unix_ms,agents_json)
             VALUES(?1,?2,?3,?4)
             ON CONFLICT(workspace_id,area_id) DO UPDATE SET
               updated_at_unix_ms=excluded.updated_at_unix_ms,
               agents_json=excluded.agents_json
             WHERE excluded.updated_at_unix_ms > desktop_area_status.updated_at_unix_ms",
            params![workspace_id, area_id, updated_at_unix_ms, agents_json],
        )?;
        Ok(changed > 0)
    }

    pub fn list(&self, workspace_id: &str) -> StorageResult<Vec<DesktopAreaStatus>> {
        let connection = open_app_connection(&self.path)?;
        let mut query = connection.prepare(
            "SELECT s.area_id,s.updated_at_unix_ms,s.agents_json
             FROM desktop_area_status s
             JOIN desktop_project_areas a
               ON a.id=s.area_id AND a.desktop_workspace_id=s.workspace_id
             WHERE s.workspace_id=?1 ORDER BY a.added_at_unix_ms,a.id",
        )?;
        let rows = query
            .query_map([workspace_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(area_id, updated_at_unix_ms, json)| {
                Ok(DesktopAreaStatus {
                    area_id,
                    updated_at_unix_ms,
                    agents: serde_json::from_str(&json)?,
                })
            })
            .collect()
    }
}
