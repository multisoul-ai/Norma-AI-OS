use crate::{
    migrations::open_app_connection, project_store::desktop_canvas_path,
    workspace_identity_store::random_workspace_id,
};
mod dispatch_effect;
pub(crate) mod mobile_configuration;
pub(crate) mod wallpaper;
pub use dispatch_effect::{DispatchEffectPreference, DispatchEffectSnapshot};
pub use wallpaper::{WallpaperReference, WallpaperUsage};

use crate::{StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use workspace::project::identify_project;

/// Local desktop canvas identity. This is deliberately separate from the
/// project-scoped `WorkspaceIdentity` used by mobile and orchestration routes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopWorkspace {
    pub id: String,
    pub name: String,
    pub created_at_unix_ms: i64,
    pub last_opened_at_unix_ms: i64,
    pub project_count: usize,
    pub expanded_area_id: Option<String>,
    pub overview_scroll_y: i64,
    pub wallpaper: Option<WallpaperReference>,
    pub drawer_layout_json: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopProjectArea {
    pub id: String,
    pub desktop_workspace_id: String,
    pub project_key: PathBuf,
    pub root: PathBuf,
    pub display_name: String,
    pub added_at_unix_ms: i64,
    pub wallpaper: Option<WallpaperReference>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesktopAreaPreview {
    pub id: String,
    pub name: String,
    pub nodes: Vec<canvas::CanvasNodeSnapshot>,
}

#[derive(Clone, Debug)]
pub struct DesktopWorkspaceStore {
    path: PathBuf,
}

const SELECT: &str = "SELECT id, name, created_at_unix_ms, last_opened_at_unix_ms, \
                      (SELECT count(*) FROM desktop_project_areas a WHERE a.desktop_workspace_id=desktop_workspaces.id) \
                      , expanded_area_id, overview_scroll_y, wallpaper_json, drawer_layout_json FROM desktop_workspaces";
const SELECT_AREA: &str =
    "SELECT id, desktop_workspace_id, project_key, root_path, display_name, added_at_unix_ms, wallpaper_json \
                           FROM desktop_project_areas";

impl DesktopWorkspaceStore {
    /// Validate the name used by creation, rename and their UI; count Unicode characters, not bytes.
    pub fn validate_name(name: &str) -> StorageResult<&str> {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.chars().count() > 80 {
            return Err(StorageError::InvalidDesktopWorkspaceName(name.to_string()));
        }
        Ok(trimmed)
    }

    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn list(&self) -> StorageResult<Vec<DesktopWorkspace>> {
        let connection = open_app_connection(&self.path)?;
        let mut statement =
            connection.prepare(&format!("{SELECT} ORDER BY created_at_unix_ms ASC, id ASC"))?;
        let rows = statement
            .query_map([], read_workspace)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into);
        rows
    }

    pub fn get(&self, id: &str) -> StorageResult<Option<DesktopWorkspace>> {
        let connection = open_app_connection(&self.path)?;
        select(&connection, id)
    }

    pub fn list_previews(&self) -> StorageResult<HashMap<String, Vec<DesktopAreaPreview>>> {
        let mut previews = HashMap::new();
        for workspace in self.list()? {
            let mut areas = Vec::new();
            for area in self.list_projects(&workspace.id)? {
                let path = desktop_canvas_path(&self.path, &workspace.id, &area.id)?;
                let snapshot = if path.exists() {
                    let canvas = crate::CanvasStore::new(path);
                    canvas.prune_desktop_content_cards()?;
                    canvas.load_snapshot()?
                } else {
                    None
                };
                let nodes = snapshot.map(|snapshot| snapshot.nodes).unwrap_or_default();
                areas.push(DesktopAreaPreview {
                    id: area.id,
                    name: area.display_name,
                    nodes,
                });
            }
            previews.insert(workspace.id, areas);
        }
        Ok(previews)
    }

    pub fn create(&self, name: &str) -> StorageResult<DesktopWorkspace> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction()?;
        let wallpaper = wallpaper::legacy_reference(&transaction)?;
        let created = self.create_on_connection(&transaction, name, wallpaper.as_ref())?;
        transaction.commit()?;
        Ok(created)
    }

    pub fn rename(&self, id: &str, name: &str) -> StorageResult<()> {
        let name = Self::validate_name(name)?;
        let connection = open_app_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE desktop_workspaces SET name=?1 WHERE id=?2",
            params![name, id],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    pub fn touch(&self, id: &str) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        let now = next_opened_at(&connection)?;
        let changed = connection.execute(
            "UPDATE desktop_workspaces SET last_opened_at_unix_ms=?1 WHERE id=?2",
            params![now, id],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    pub fn set_expanded_area(
        &self,
        workspace_id: &str,
        area_id: Option<&str>,
    ) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE desktop_workspaces SET expanded_area_id=?2 WHERE id=?1 AND
             (?2 IS NULL OR EXISTS(SELECT 1 FROM desktop_project_areas WHERE id=?2 AND desktop_workspace_id=?1))",
            params![workspace_id, area_id],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    pub fn set_drawer_layout(&self, workspace_id: &str, layout_json: &str) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE desktop_workspaces SET drawer_layout_json=?2 WHERE id=?1",
            params![workspace_id, layout_json],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    pub fn set_overview_scroll_y(&self, workspace_id: &str, offset: i64) -> StorageResult<()> {
        let connection = open_app_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE desktop_workspaces SET overview_scroll_y=?2 WHERE id=?1",
            params![workspace_id, offset],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    pub fn delete(&self, id: &str) -> StorageResult<bool> {
        let Some(area_ids) = self.delete_catalog(id)? else {
            return Ok(false);
        };
        for area_id in area_ids {
            self.remove_canvas_file(id, &area_id)?;
        }
        Ok(true)
    }

    /// Commit the catalog change before callers stop live sessions and remove their canvas files.
    pub fn delete_catalog(&self, id: &str) -> StorageResult<Option<Vec<String>>> {
        let area_ids = self
            .list_projects(id)?
            .into_iter()
            .map(|area| area.id)
            .collect();
        let connection = open_app_connection(&self.path)?;
        let removed = connection.execute("DELETE FROM desktop_workspaces WHERE id=?1", [id])? > 0;
        Ok(removed.then_some(area_ids))
    }

    pub fn list_projects(&self, workspace_id: &str) -> StorageResult<Vec<DesktopProjectArea>> {
        let connection = open_app_connection(&self.path)?;
        let mut statement = connection.prepare(&format!(
            "{SELECT_AREA} WHERE desktop_workspace_id=?1 ORDER BY added_at_unix_ms, id"
        ))?;
        let rows = statement
            .query_map([workspace_id], read_area)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn add_project(
        &self,
        workspace_id: &str,
        path: &std::path::Path,
    ) -> StorageResult<DesktopProjectArea> {
        let project = identify_project(path)?;
        let project_key = project.key.to_string_lossy().into_owned();
        let connection = open_app_connection(&self.path)?;
        let id = random_workspace_id()?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;
        connection.execute(
            "INSERT INTO desktop_project_areas
             (id, desktop_workspace_id, project_key, root_path, display_name, added_at_unix_ms)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                workspace_id,
                project_key,
                project.root.to_string_lossy(),
                workspace::display_name_for_path(&project.root),
                now,
            ],
        )?;
        connection
            .query_row(&format!("{SELECT_AREA} WHERE id=?1"), [id], read_area)
            .map_err(Into::into)
    }

    pub fn remove_project(&self, workspace_id: &str, area_id: &str) -> StorageResult<bool> {
        let removed = self.remove_project_catalog(workspace_id, area_id)?;
        if removed {
            self.remove_canvas_file(workspace_id, area_id)?;
        }
        Ok(removed)
    }

    pub fn rename_project(
        &self,
        workspace_id: &str,
        area_id: &str,
        name: &str,
    ) -> StorageResult<()> {
        let name = Self::validate_name(name)?;
        let connection = open_app_connection(&self.path)?;
        let changed = connection.execute(
            "UPDATE desktop_project_areas SET display_name=?3 WHERE desktop_workspace_id=?1 AND id=?2",
            params![workspace_id, area_id, name],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        Ok(())
    }

    /// Commit the catalog change before callers stop the area's live session.
    pub fn remove_project_catalog(&self, workspace_id: &str, area_id: &str) -> StorageResult<bool> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction()?;
        let removed = transaction.execute(
            "DELETE FROM desktop_project_areas WHERE desktop_workspace_id=?1 AND id=?2",
            params![workspace_id, area_id],
        )? > 0;
        if removed {
            transaction.execute("UPDATE desktop_workspaces SET expanded_area_id=NULL WHERE id=?1 AND expanded_area_id=?2", params![workspace_id, area_id])?;
        }
        transaction.commit()?;
        Ok(removed)
    }

    pub fn remove_canvas_file(&self, workspace_id: &str, area_id: &str) -> StorageResult<()> {
        let path = desktop_canvas_path(&self.path, workspace_id, area_id)?;
        for candidate in [
            path.clone(),
            path.with_extension("sqlite-wal"),
            path.with_extension("sqlite-shm"),
        ] {
            match std::fs::remove_file(candidate) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

fn next_opened_at(connection: &Connection) -> StorageResult<i64> {
    let current = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;
    let latest: Option<i64> = connection.query_row(
        "SELECT MAX(last_opened_at_unix_ms) FROM desktop_workspaces",
        [],
        |row| row.get(0),
    )?;
    Ok(current.max(latest.unwrap_or_default() + 1))
}

fn select(connection: &Connection, id: &str) -> StorageResult<Option<DesktopWorkspace>> {
    connection
        .query_row(&format!("{SELECT} WHERE id=?1"), [id], read_workspace)
        .optional()
        .map_err(Into::into)
}

fn read_workspace(row: &Row<'_>) -> rusqlite::Result<DesktopWorkspace> {
    Ok(DesktopWorkspace {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at_unix_ms: row.get(2)?,
        last_opened_at_unix_ms: row.get(3)?,
        project_count: row.get(4)?,
        expanded_area_id: row.get(5)?,
        overview_scroll_y: row.get(6)?,
        wallpaper: wallpaper::decode(row.get(7)?),
        drawer_layout_json: row.get(8)?,
    })
}

fn read_area(row: &Row<'_>) -> rusqlite::Result<DesktopProjectArea> {
    Ok(DesktopProjectArea {
        id: row.get(0)?,
        desktop_workspace_id: row.get(1)?,
        project_key: PathBuf::from(row.get::<_, String>(2)?),
        root: PathBuf::from(row.get::<_, String>(3)?),
        display_name: row.get(4)?,
        added_at_unix_ms: row.get(5)?,
        wallpaper: wallpaper::decode(row.get(6)?),
    })
}
