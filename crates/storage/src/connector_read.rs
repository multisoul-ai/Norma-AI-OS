//! Background-only connector projection. Open existing files read-only; never creates a Runtime.
use crate::{project_store::desktop_canvas_path, DesktopProjectArea, StorageResult};
use canvas::{CanvasNodeKind, DocumentSnapshot};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct ConnectorWorkspace {
    pub id: String,
    pub name: String,
    pub areas: Vec<ConnectorArea>,
}
pub struct ConnectorArea {
    pub area: DesktopProjectArea,
    pub canvas_id: String,
    pub members: Vec<ConnectorMember>,
    pub definition_births: std::collections::HashMap<i64, i64>,
    pub tasks: Vec<ConnectorTask>,
}
pub struct ConnectorMember {
    pub node_id: u64,
    pub definition_id: i64,
    pub name: String,
    pub profile: String,
    pub created_at: i64,
    pub definition_created_at: Option<i64>,
}
pub struct ConnectorTask {
    pub id: i64,
    pub task_ref: String,
    pub node_id: u64,
    pub definition_id: i64,
    pub state: String,
    pub tracking: String,
    pub summary: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub error: Option<String>,
    pub result_summary: Option<String>,
    pub events: Vec<(i64, String)>,
}
#[derive(Clone)]
pub struct ConnectorReadStore {
    path: PathBuf,
}
impl ConnectorReadStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn read(&self) -> StorageResult<Vec<ConnectorWorkspace>> {
        let conn = readonly(&self.path)?;
        let mut stmt =
            conn.prepare("SELECT id,name FROM desktop_workspaces ORDER BY created_at_unix_ms,id")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut spaces = Vec::new();
        for (id, name) in rows {
            let mut stmt = conn.prepare("SELECT id,project_key,root_path,display_name,added_at_unix_ms FROM desktop_project_areas WHERE desktop_workspace_id=?1 ORDER BY added_at_unix_ms,id")?;
            let areas = stmt
                .query_map([&id], |r| {
                    Ok(DesktopProjectArea {
                        id: r.get(0)?,
                        desktop_workspace_id: id.clone(),
                        project_key: PathBuf::from(r.get::<_, String>(1)?),
                        root: PathBuf::from(r.get::<_, String>(2)?),
                        display_name: r.get(3)?,
                        added_at_unix_ms: r.get(4)?,
                        wallpaper: None,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let mut snapshots = Vec::new();
            for area in areas {
                snapshots.push(self.area(area)?);
            }
            spaces.push(ConnectorWorkspace {
                id,
                name,
                areas: snapshots,
            });
        }
        Ok(spaces)
    }
    fn area(&self, area: DesktopProjectArea) -> StorageResult<ConnectorArea> {
        let path = desktop_canvas_path(&self.path, &area.desktop_workspace_id, &area.id)?;
        // This filename already is the persistent, per-area canvas identity on dev.
        let canvas_id = path
            .file_stem()
            .expect("canvas filename")
            .to_string_lossy()
            .into_owned();
        let mut result = ConnectorArea {
            area,
            canvas_id,
            members: Vec::new(),
            definition_births: Default::default(),
            tasks: Vec::new(),
        };
        if !path.is_file() {
            return Ok(result);
        }
        let conn = readonly(&path)?;
        let json: Option<String> = conn
            .query_row("SELECT json FROM canvas_snapshots WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let document: Option<DocumentSnapshot> =
            json.map(|j| serde_json::from_str(&j)).transpose()?;
        let definitions = self
            .path
            .parent()
            .expect("App parent")
            .join("desktop-project-definitions")
            .join(format!(
                "{:x}.sqlite",
                Sha256::digest(result.area.project_key.to_string_lossy().as_bytes())
            ));
        let defs = definitions
            .is_file()
            .then(|| readonly(&definitions))
            .transpose()?;
        if let Some(defs) = &defs {
            result.definition_births = defs
                .prepare("SELECT id,created_at_unix FROM agent_definitions")?
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
        }
        for node in document.into_iter().flat_map(|d| d.nodes) {
            let CanvasNodeKind::Agent(spec) = node.kind else {
                continue;
            };
            let id = spec.definition_id.unwrap_or(0);
            let member = if let Some(defs) = &defs {
                defs.query_row("SELECT name,runtime_profile,created_at_unix FROM agent_definitions WHERE id=?1",[id],|r|Ok(ConnectorMember { node_id:node.id,definition_id:id,name:r.get(0)?,profile:r.get(1)?,created_at:r.get(2)?,definition_created_at:Some(r.get(2)?) })).optional()?
            } else {
                None
            };
            result.members.push(member.unwrap_or_else(|| {
                ConnectorMember {
                    node_id: node.id,
                    definition_id: id,
                    name: spec
                        .alias
                        .unwrap_or_else(|| spec.profile.display_title().into()),
                    profile: spec.profile.display_title().into(),
                    created_at: result.area.added_at_unix_ms / 1000,
                    definition_created_at: None,
                }
            }));
        }
        let mut stmt = conn.prepare("SELECT id,task_ref,node_id,definition_id,state,tracking,substr(task_text,1,500),created_at_unix,updated_at_unix,substr(error,1,500),CASE WHEN result_captured=1 THEN substr(result_text,1,500) END FROM agent_dispatch_tasks ORDER BY created_at_unix,id")?;
        result.tasks = stmt
            .query_map([], |r| {
                Ok(ConnectorTask {
                    id: r.get(0)?,
                    task_ref: r.get(1)?,
                    node_id: r.get(2)?,
                    definition_id: r.get(3)?,
                    state: r.get(4)?,
                    tracking: r.get(5)?,
                    summary: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                    error: r.get(9)?,
                    result_summary: r.get(10)?,
                    events: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let has_events: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='agent_dispatch_task_events' AND type='table')", [], |r|r.get(0))?;
        if has_events {
            for task in &mut result.tasks {
                task.events = conn.prepare("SELECT occurred_at_unix,state FROM agent_dispatch_task_events WHERE task_id=?1 ORDER BY sequence DESC LIMIT 20")?.query_map([task.id], |r|Ok((r.get(0)?,r.get(1)?)))?.collect::<Result<Vec<_>,_>>()?;
            }
        }
        Ok(result)
    }
}
fn readonly(path: &Path) -> StorageResult<Connection> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}
