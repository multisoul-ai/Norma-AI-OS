//! Background-only catalog reader. Never migrates, registers, restores or starts a PTY.
use crate::{
    project_store::desktop_canvas_path, StorageResult, WorkspaceAgentRoleStore,
    WorkspaceCallKeyState, WorkspaceRoleSnapshot,
};
use agent::workspace_dispatch::*;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::PathBuf, time::Duration};

mod area;

#[derive(Clone, Debug)]
pub struct WorkspaceDirectoryStore {
    path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CatalogArea {
    id: String,
    project_id: String,
    name: String,
    root: String,
}

impl WorkspaceDirectoryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// `None` is the desktop's complete local scope. Mobile callers must pass the
    /// exact authorized area IDs and later revalidate the grant before accepting work.
    /// Returns only authorized metadata, even when another area is unreadable.
    pub fn read(
        &self,
        workspace: &str,
        authorization_revision: &str,
        authorized: Option<&[String]>,
    ) -> StorageResult<WorkspaceDirectory> {
        let mut result = WorkspaceDirectory {
            workspace_id: workspace.into(),
            revision: 0,
            catalog_fingerprint: String::new(),
            authorization_revision: authorization_revision.into(),
            authorized_area_ids: authorized.unwrap_or_default().to_vec(),
            covered_area_ids: vec![],
            missing_area_ids: vec![],
            registration_complete: false,
            projects: vec![],
            areas: vec![],
            roles: vec![],
            call_keys: vec![],
        };
        let Some(mut connection) = readonly(&self.path)? else {
            return Ok(result);
        };
        if !tables_present(
            &connection,
            &[
                "desktop_workspaces",
                "desktop_project_areas",
                "workspace_agent_roles",
                "workspace_agent_call_keys",
                "workspace_agent_role_revisions",
            ],
        )? {
            return Ok(result);
        }
        let roles = WorkspaceAgentRoleStore::new(self.path.clone()).snapshot(workspace)?;
        let tx = connection.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM desktop_workspaces WHERE id=?1)",
            [workspace],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(result);
        }
        let catalog = read_catalog(&tx, workspace, authorized)?;
        if authorized.is_none() {
            result.authorized_area_ids = catalog.iter().map(|a| a.id.clone()).collect();
        }
        result.revision = roles.revision;
        let catalog_value: Vec<_> = catalog
            .iter()
            .map(|a| (&a.id, &a.project_id, &a.name, &a.root))
            .collect();
        result.catalog_fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                workspace,
                &result.authorized_area_ids,
                catalog_value
            ))?)
        );
        result.registration_complete =
            roles.schema_present && revision(&tx, workspace)? == roles.revision;
        for area in &catalog {
            let project = WorkspaceProjectRef {
                workspace_id: workspace.into(),
                project_id: area.project_id.clone(),
            };
            result.areas.push(WorkspaceDirectoryArea {
                area_id: area.id.clone(),
                project: project.clone(),
                name: area.name.clone(),
            });
            if let Some(p) = result.projects.iter_mut().find(|p| p.reference == project) {
                p.area_ids.push(area.id.clone());
            } else {
                result.projects.push(WorkspaceDirectoryProject {
                    reference: project.clone(),
                    name: area.name.clone(),
                    area_ids: vec![area.id.clone()],
                });
            }
            match self.read_area(workspace, area, &project, &roles) {
                Ok(Some(area_roles)) => {
                    result.covered_area_ids.push(area.id.clone());
                    result.roles.extend(area_roles);
                }
                Ok(None) | Err(_) => result.missing_area_ids.push(area.id.clone()),
            }
        }
        // Unknown/stale authorized IDs are missing, never silently dropped from scope.
        for id in &result.authorized_area_ids {
            if !catalog.iter().any(|a| a.id == *id) {
                result.missing_area_ids.push(id.clone());
            }
        }
        let present: HashSet<_> = result
            .roles
            .iter()
            .map(|r| &r.card.card_instance_id)
            .collect();
        result.call_keys = roles
            .call_keys
            .iter()
            .filter_map(|key| {
                if !present.contains(&key.card_instance_id) {
                    return None;
                }
                let card = result
                    .roles
                    .iter()
                    .find(|r| r.card.card_instance_id == key.card_instance_id)?
                    .card
                    .clone();
                Some(WorkspaceDirectoryCallKey {
                    key: key.normalized_key.clone(),
                    card,
                    state: if key.state == WorkspaceCallKeyState::Active {
                        WorkspaceCallKeyStatus::Active
                    } else {
                        WorkspaceCallKeyStatus::Retired
                    },
                })
            })
            .collect();
        tx.commit()?;
        // Catalog and role revision can change while separate area databases are sampled.
        // A fresh read marks the snapshot incomplete instead of publishing a guessed unique target.
        if read_catalog(&connection, workspace, authorized)? != catalog
            || revision(&connection, workspace)? != roles.revision
        {
            result.registration_complete = false;
        }
        if !result.is_consistent() {
            result.registration_complete = false;
        }
        Ok(result)
    }
}

fn read_catalog(
    connection: &Connection,
    workspace: &str,
    authorized: Option<&[String]>,
) -> StorageResult<Vec<CatalogArea>> {
    let mut statement = connection.prepare("SELECT id,project_key,display_name,root_path FROM desktop_project_areas WHERE desktop_workspace_id=?1 ORDER BY added_at_unix_ms,id")?;
    let rows = statement
        .query_map([workspace], |row| {
            Ok(CatalogArea {
                id: row.get(0)?,
                project_id: row.get(1)?,
                name: row.get(2)?,
                root: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter(|a| authorized.is_none_or(|scope| scope.contains(&a.id)))
        .collect())
}
fn revision(connection: &Connection, workspace: &str) -> StorageResult<u64> {
    Ok(connection
        .query_row(
            "SELECT revision FROM workspace_agent_role_revisions WHERE workspace_id=?1",
            [workspace],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}
fn readonly(path: &std::path::Path) -> StorageResult<Option<Connection>> {
    if !path.try_exists()? {
        return Ok(None);
    }
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    c.busy_timeout(Duration::from_secs(5))?;
    Ok(Some(c))
}
fn tables_present(connection: &Connection, tables: &[&str]) -> StorageResult<bool> {
    for table in tables {
        if !connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |row| row.get::<_, bool>(0),
        )? {
            return Ok(false);
        }
    }
    Ok(true)
}
