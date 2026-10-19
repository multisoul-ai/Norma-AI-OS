use crate::{
    migrations::open_app_connection, workspace_identity_store::random_workspace_id, StorageResult,
};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopContentKind {
    Browser,
    ProjectFiles,
    File,
}

impl DesktopContentKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::ProjectFiles => "project_files",
            Self::File => "file",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewDesktopContentTab {
    pub workspace_id: String,
    pub source_area_id: Option<String>,
    pub source_project_name: String,
    pub kind: DesktopContentKind,
    pub resource: String,
    pub version: Option<String>,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopContentTab {
    pub id: String,
    pub workspace_id: String,
    pub source_area_id: Option<String>,
    pub source_project_name: String,
    pub kind: DesktopContentKind,
    pub resource: String,
    pub version: Option<String>,
    pub title: String,
    pub position: i64,
    pub selected: bool,
    pub source_removed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopBrowserLocationUpdate {
    pub changed: bool,
    pub removed_tab_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DesktopContentTabStore {
    path: PathBuf,
}

const SELECT_TAB: &str = "SELECT t.id,t.workspace_id,t.source_area_id,t.source_project_name,\
    t.kind,t.resource,t.version,t.title,t.position,t.selected,\
    CASE WHEN t.source_area_id IS NULL THEN 0 WHEN a.id IS NULL THEN 1 ELSE 0 END \
    FROM desktop_content_tabs t LEFT JOIN desktop_project_areas a \
    ON a.desktop_workspace_id=t.workspace_id AND a.id=t.source_area_id";

impl DesktopContentTabStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn state(&self, workspace_id: &str) -> StorageResult<Option<String>> {
        open_app_connection(&self.path)?
            .query_row(
                "SELECT state_json FROM desktop_content_state WHERE workspace_id=?1",
                [workspace_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn save_state(&self, workspace_id: &str, state: &str) -> StorageResult<()> {
        open_app_connection(&self.path)?.execute(
            "INSERT INTO desktop_content_state(workspace_id,state_json) VALUES(?1,?2) ON CONFLICT(workspace_id) DO UPDATE SET state_json=excluded.state_json",
            params![workspace_id, state],
        )?;
        Ok(())
    }

    pub fn list(&self, workspace_id: &str) -> StorageResult<Vec<DesktopContentTab>> {
        let connection = open_app_connection(&self.path)?;
        let mut statement = connection.prepare(&format!(
            "{SELECT_TAB} WHERE t.workspace_id=?1 ORDER BY t.position,t.id"
        ))?;
        let tabs = statement
            .query_map([workspace_id], read_tab)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tabs)
    }

    pub fn open_or_select(
        &self,
        request: NewDesktopContentTab,
    ) -> StorageResult<DesktopContentTab> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction()?;
        let kind = request.kind.as_str();
        let version = request.version.as_deref().unwrap_or("");
        let existing: Option<String> = transaction
            .query_row(
                "SELECT id FROM desktop_content_tabs WHERE workspace_id=?1 AND kind=?2 AND resource=?3 AND version=?4",
                params![request.workspace_id, kind, request.resource, version],
                |row| row.get(0),
            )
            .optional()?;
        transaction.execute(
            "UPDATE desktop_content_tabs SET selected=0 WHERE workspace_id=?1 AND selected=1",
            [&request.workspace_id],
        )?;
        let id = if let Some(id) = existing {
            transaction.execute(
                "UPDATE desktop_content_tabs SET source_area_id=?2,source_project_name=?3,title=?4,selected=1 WHERE id=?1",
                params![id, request.source_area_id, request.source_project_name, request.title],
            )?;
            id
        } else {
            let id = random_workspace_id()?;
            let position: i64 = transaction.query_row(
                "SELECT COALESCE(MAX(position)+1,0) FROM desktop_content_tabs WHERE workspace_id=?1",
                [&request.workspace_id],
                |row| row.get(0),
            )?;
            transaction.execute(
                "INSERT INTO desktop_content_tabs(id,workspace_id,source_area_id,source_project_name,kind,resource,version,title,position,selected) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,1)",
                params![id, request.workspace_id, request.source_area_id, request.source_project_name, kind, request.resource, version, request.title, position],
            )?;
            id
        };
        let tab = load_tab(&transaction, &request.workspace_id, &id)?;
        transaction.commit()?;
        Ok(tab)
    }

    pub fn select(&self, workspace_id: &str, tab_id: &str) -> StorageResult<()> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction()?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM desktop_content_tabs WHERE workspace_id=?1 AND id=?2)",
            params![workspace_id, tab_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        transaction.execute(
            "UPDATE desktop_content_tabs SET selected=0 WHERE workspace_id=?1 AND selected=1",
            [workspace_id],
        )?;
        transaction.execute(
            "UPDATE desktop_content_tabs SET selected=1 WHERE workspace_id=?1 AND id=?2",
            params![workspace_id, tab_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn select_or_matching_browser(
        &self,
        workspace_id: &str,
        tab_id: &str,
        resource: &str,
        version: Option<&str>,
    ) -> StorageResult<bool> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction()?;
        let target: Option<String> = transaction
            .query_row(
                "SELECT id FROM desktop_content_tabs WHERE workspace_id=?1 AND id=?2",
                params![workspace_id, tab_id],
                |row| row.get(0),
            )
            .optional()?
            .or(transaction
                .query_row(
                    "SELECT id FROM desktop_content_tabs WHERE workspace_id=?1 AND kind='browser' AND resource=?2 AND version=?3",
                    params![workspace_id, resource, version.unwrap_or("")],
                    |row| row.get(0),
                )
                .optional()?);
        let Some(target) = target else {
            return Ok(false);
        };
        transaction.execute(
            "UPDATE desktop_content_tabs SET selected=0 WHERE workspace_id=?1 AND selected=1",
            [workspace_id],
        )?;
        transaction.execute(
            "UPDATE desktop_content_tabs SET selected=1 WHERE workspace_id=?1 AND id=?2",
            params![workspace_id, target],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn update_browser_location(
        &self,
        workspace_id: &str,
        tab_id: &str,
        resource: &str,
        title: &str,
    ) -> StorageResult<DesktopBrowserLocationUpdate> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction()?;
        let current: Option<(String, bool, String)> = transaction
            .query_row(
                "SELECT resource,selected,version FROM desktop_content_tabs WHERE workspace_id=?1 AND id=?2 AND kind='browser'",
                params![workspace_id, tab_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((current_resource, current_selected, version)) = current else {
            return Ok(DesktopBrowserLocationUpdate {
                changed: false,
                removed_tab_id: None,
            });
        };
        if current_resource == resource {
            return Ok(DesktopBrowserLocationUpdate {
                changed: false,
                removed_tab_id: None,
            });
        }
        let duplicate: Option<(String, bool)> = transaction
            .query_row(
                "SELECT id,selected FROM desktop_content_tabs WHERE workspace_id=?1 AND kind='browser' AND resource=?2 AND id<>?3 AND version=?4",
                params![workspace_id, resource, tab_id, version],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((duplicate_id, duplicate_selected)) = &duplicate {
            if *duplicate_selected && !current_selected {
                transaction.execute(
                    "DELETE FROM desktop_content_tabs WHERE workspace_id=?1 AND id=?2",
                    params![workspace_id, tab_id],
                )?;
                transaction.commit()?;
                return Ok(DesktopBrowserLocationUpdate {
                    changed: true,
                    removed_tab_id: Some(tab_id.to_owned()),
                });
            }
            transaction.execute(
                "DELETE FROM desktop_content_tabs WHERE workspace_id=?1 AND id=?2",
                params![workspace_id, duplicate_id],
            )?;
        }
        transaction.execute(
            "UPDATE desktop_content_tabs SET resource=?3,title=?4 WHERE workspace_id=?1 AND id=?2",
            params![workspace_id, tab_id, resource, title],
        )?;
        transaction.commit()?;
        Ok(DesktopBrowserLocationUpdate {
            changed: true,
            removed_tab_id: duplicate.map(|(id, _)| id),
        })
    }

    pub fn close(&self, workspace_id: &str, tab_id: &str) -> StorageResult<bool> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction()?;
        let selected: Option<bool> = transaction
            .query_row(
                "SELECT selected FROM desktop_content_tabs WHERE workspace_id=?1 AND id=?2",
                params![workspace_id, tab_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(selected) = selected else {
            return Ok(false);
        };
        transaction.execute(
            "DELETE FROM desktop_content_tabs WHERE workspace_id=?1 AND id=?2",
            params![workspace_id, tab_id],
        )?;
        if selected {
            transaction.execute(
                "UPDATE desktop_content_tabs SET selected=1 WHERE id=(SELECT id FROM desktop_content_tabs WHERE workspace_id=?1 ORDER BY position DESC,id DESC LIMIT 1)",
                [workspace_id],
            )?;
        }
        transaction.commit()?;
        Ok(true)
    }
}

fn load_tab(
    connection: &Connection,
    workspace_id: &str,
    id: &str,
) -> StorageResult<DesktopContentTab> {
    let tab = connection.query_row(
        &format!("{SELECT_TAB} WHERE t.workspace_id=?1 AND t.id=?2"),
        params![workspace_id, id],
        read_tab,
    )?;
    Ok(tab)
}

fn read_tab(row: &Row<'_>) -> rusqlite::Result<DesktopContentTab> {
    let kind: String = row.get(4)?;
    let kind = match kind.as_str() {
        "browser" => DesktopContentKind::Browser,
        "project_files" => DesktopContentKind::ProjectFiles,
        "file" => DesktopContentKind::File,
        _ => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                format!("invalid desktop content kind: {kind}").into(),
            ))
        }
    };
    let version: String = row.get(6)?;
    Ok(DesktopContentTab {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        source_area_id: row.get(2)?,
        source_project_name: row.get(3)?,
        kind,
        resource: row.get(5)?,
        version: (!version.is_empty()).then_some(version),
        title: row.get(7)?,
        position: row.get(8)?,
        selected: row.get(9)?,
        source_removed: row.get(10)?,
    })
}
