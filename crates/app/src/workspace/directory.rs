//! Read the frozen source workspace without loading a canvas or writing card input.
use crate::app::MultisoulApp;
use gpui::{App, Context, Task};
use norma::workspace_dispatch::*;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path};

impl MultisoulApp {
    /// Callers freeze the source workspace and an already-authorized mobile scope.
    /// This snapshot does not replace grant/catalog/identity revalidation at acceptance.
    pub fn read_workspace_dispatch_directory(
        &self,
        workspace_id: String,
        authorization_revision: String,
        authorized_area_ids: Option<Vec<String>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<WorkspaceDirectory, String>> {
        let database = self.database_paths.app_db_path().to_owned();
        let read_path = database.clone();
        cx.spawn(async move |_, cx| {
            let mut directory = cx
                .background_executor()
                .spawn(async move {
                    storage::WorkspaceDirectoryStore::new(read_path)
                        .read(
                            &workspace_id,
                            &authorization_revision,
                            authorized_area_ids.as_deref(),
                        )
                        .map_err(|error| error.to_string())
                })
                .await?;
            cx.update(|cx| project_retained_directory(&database, &mut directory, cx))
                .map_err(|error| error.to_string())?;
            Ok(directory)
        })
    }

    pub(crate) fn workspace_dispatch_route_generation(&self) -> u64 {
        self.workspace_route_id.as_ref().map_or(0, |route| {
            let hash = Sha256::digest(route.as_bytes());
            u64::from_le_bytes(hash[..8].try_into().expect("SHA256 has eight prefix bytes"))
        })
    }
}

pub(crate) fn project_retained_directory(
    database: &Path,
    directory: &mut WorkspaceDirectory,
    cx: &App,
) {
    let hosts = super::desktop::sessions::hosts(database, cx);
    // The explicit source survives active-workspace and expanded-project changes.
    for area in directory.authorized_area_ids.clone() {
        let key = (directory.workspace_id.clone(), area.clone());
        let sessions: Vec<_> = hosts
            .iter()
            .filter_map(|host| host.read(cx).desktop_workspace.sessions.get(&key).cloned())
            .collect();
        match sessions.as_slice() {
            [] => {} // Persisted cold cards are already present; never ensure a session.
            [session] => {
                let app = session.read(cx);
                if app.workspace_role_scope()
                    != Some((directory.workspace_id.as_str(), area.as_str()))
                {
                    mark_missing(directory, &area);
                    continue;
                }
                project_runtime_directory(
                    directory,
                    &area,
                    &app.runtime,
                    app.agent_definitions.definitions(),
                    app.workspace_dispatch_route_generation(),
                );
            }
            _ => mark_missing(directory, &area), // Multiple runtime owners are not interchangeable.
        }
    }
}

/// Pure in-memory projection. Only exact persisted cards can receive a live fingerprint.
pub(crate) fn project_runtime_directory(
    directory: &mut WorkspaceDirectory,
    area: &str,
    runtime: &runtime::Runtime,
    definitions: &[storage::AgentDefinition],
    route_generation: u64,
) {
    if !directory.authorized_area_ids.iter().any(|id| id == area) {
        directory.registration_complete = false;
        return;
    }
    let expected: HashSet<_> = directory
        .roles
        .iter()
        .filter(|role| role.card.area_id == area)
        .map(|role| role.card.clone())
        .collect();
    for role in directory
        .roles
        .iter_mut()
        .filter(|role| role.card.area_id == area)
    {
        role.execution = None;
    }
    let mut observed = HashSet::new();
    let mut missing = false;
    for node in runtime.document.nodes() {
        let canvas::NodeKind::Agent(spec) = &node.kind else {
            continue;
        };
        let Ok(target) =
            runtime.workspace_card_target(&directory.workspace_id, area, route_generation, node.id)
        else {
            missing = true;
            continue;
        };
        if !expected.contains(&target.card) || !observed.insert(target.card.clone()) {
            missing = true;
            continue;
        }
        let role = directory
            .roles
            .iter_mut()
            .find(|role| role.card == target.card)
            .expect("the exact card was in the expected directory set");
        if let Some(id) = spec.definition_id {
            let Some(definition) = definitions.iter().find(|definition| definition.id == id) else {
                missing = true;
                continue;
            };
            // In-memory disabling can narrow the persisted scope, never enable a stored disabled role.
            role.enabled &= definition.enabled;
        }
        if target.execution.terminal_session_id.is_some() {
            role.execution = Some(target.execution);
        }
    }
    if missing || observed != expected {
        mark_missing(directory, area);
    }
}

fn mark_missing(directory: &mut WorkspaceDirectory, area: &str) {
    directory.covered_area_ids.retain(|id| id != area);
    if !directory.missing_area_ids.iter().any(|id| id == area) {
        directory.missing_area_ids.push(area.into());
    }
    for role in directory
        .roles
        .iter_mut()
        .filter(|role| role.card.area_id == area)
    {
        role.execution = None;
    }
}
