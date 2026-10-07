use super::*;

/// Filter each project using the existing device grant; default enrollment grants no new access.
pub(in crate::mobile) fn areas(
    path: &Path,
    workspace: Option<&str>,
    grant: &MobileDeviceGrant,
    control: bool,
) -> Result<Vec<WorkspaceArea>, &'static str> {
    let app = storage::AppStore::new(path.to_owned());
    let catalog = DesktopWorkspaceStore::new(path.to_owned());
    let spaces = if let Some(id) = workspace {
        vec![catalog
            .get(id)
            .map_err(|_| "unavailable")?
            .ok_or("workspace_missing")?]
    } else {
        catalog.list().map_err(|_| "unavailable")?
    };
    let mut result = vec![];
    for space in spaces {
        for area in catalog
            .list_projects(&space.id)
            .map_err(|_| "unavailable")?
        {
            let id = app
                .workspace_identity_store()
                .resolve_or_create(&area.root, &area.display_name)
                .map_err(|_| "configuration_changed")?;
            if grant.allows(&id.workspace_id, control) {
                result.push(WorkspaceArea {
                    area,
                    project_id: id.workspace_id,
                });
            }
        }
    }
    Ok(result)
}
pub(in crate::mobile) fn current(
    path: &Path,
    workspace: Option<&str>,
    grant: &MobileDeviceGrant,
    control: bool,
    captured: &[WorkspaceArea],
) -> bool {
    areas(path, workspace, grant, control).is_ok_and(|live| {
        live.len() == captured.len()
            && live
                .iter()
                .zip(captured)
                .all(|(a, b)| a.area == b.area && a.project_id == b.project_id)
    })
}
pub(super) fn summary(
    shared: &Shared,
    path: &Path,
    machine: &str,
    areas: &[WorkspaceArea],
) -> Result<WorkspaceActivitySummary, &'static str> {
    let mut facts = vec![];
    let mut index = 0;
    while index < areas.len() {
        let space = &areas[index].area.desktop_workspace_id;
        let end = index
            + areas[index..]
                .iter()
                .take_while(|a| &a.area.desktop_workspace_id == space)
                .count();
        facts.push(super::facts(shared, path, space, &areas[index..end])?);
        index = end;
    }
    Ok(WorkspaceActivitySummary::from_workspaces(
        machine,
        facts,
        now()?,
    ))
}
pub(super) fn now() -> Result<u64, &'static str> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| "unavailable")
}
