//! Background-only preparation of the App-wide brain store.
use super::config::{self, BrainSettings};
use brain::migration::{self, WorkspaceSource};
use std::path::{Path, PathBuf};

pub(super) struct Prepared {
    pub directory: PathBuf,
    pub settings: BrainSettings,
}

pub(super) fn prepare(
    base: &Path,
    database: PathBuf,
    mut opened: Vec<WorkspaceSource>,
) -> Result<Prepared, String> {
    brain::persistence::shared_writer()
        .flush()
        .recv()
        .map_err(|error| error.to_string())??;
    let identities = storage::WorkspaceIdentityStore::new(database.clone())
        .list()
        .map_err(|error| error.to_string())?;
    opened.extend(identities.into_iter().map(|identity| WorkspaceSource {
        workspace_id: identity.workspace_id,
        root: identity.canonical_root,
    }));
    let catalog = storage::DesktopWorkspaceStore::new(database);
    for workspace in catalog.list().map_err(|error| error.to_string())? {
        for area in catalog
            .list_projects(&workspace.id)
            .map_err(|error| error.to_string())?
        {
            opened.push(WorkspaceSource {
                workspace_id: format!("desktop:{}:{}", workspace.id, area.id),
                root: area.root,
            });
        }
    }
    let report = migration::migrate(base, &opened).map_err(|error| error.to_string())?;
    if !report.archived_workspaces.is_empty() {
        tracing::info!(workspaces = ?report.archived_workspaces, "Preserved unmapped brain workspaces in global archive");
    }
    let settings =
        config::load_settings_from(&report.directory).map_err(|error| error.to_string())?;
    Ok(Prepared {
        directory: report.directory,
        settings,
    })
}
