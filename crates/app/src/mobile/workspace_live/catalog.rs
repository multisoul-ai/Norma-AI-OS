use super::*;
pub(crate) fn catalog(database: &Path, areas: &[WorkspaceArea], cx: &App) -> Result<Value, String> {
    let mut projects = Vec::new();
    for bound in areas {
        let session = crate::workspace::desktop::mobile::session(database, &bound.area, cx)
            .ok_or("project_unavailable")?;
        let app = session.read(cx);
        let agents = app
            .mobile_card_snapshots()
            .into_iter()
            .filter_map(|card| {
                let node = app.runtime.document.node(card.node_id)?;
                let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                let id = spec.definition_id?;
                let definition = app
                    .agent_definitions
                    .definitions()
                    .iter()
                    .find(|d| d.id == id)?;
                Some(DesktopChatAgent {
                    definition_id: id,
                    name: definition.name.clone(),
                    profile: card.profile.clone(),
                    card,
                })
            })
            .collect();
        let definitions = app
            .agent_definitions
            .definitions()
            .iter()
            .map(|d| DesktopChatDefinition {
                id: d.id,
                name: d.name.clone(),
                profile: format!("{:?}", d.runtime_profile).to_lowercase(),
                enabled: d.enabled,
            })
            .collect();
        let project = DesktopChatProject {
            area_id: bound.area.id.clone(),
            project_id: bound.project_id.clone(),
            name: bound.area.display_name.clone(),
            root: bound.area.root.to_string_lossy().into_owned(),
            agents,
            definitions,
        };
        projects.push(
            json!({"desktop_workspace_id":bound.area.desktop_workspace_id,"project":project}),
        );
    }
    Ok(Value::Array(projects))
}

pub(super) fn configure_agents(
    database: &Path,
    areas: &[WorkspaceArea],
    change: WorkspaceConfigurationChange,
    cx: &mut App,
) -> Result<Value, String> {
    let area_id = match &change {
        WorkspaceConfigurationChange::AddAgent { area_id, .. }
        | WorkspaceConfigurationChange::RemoveAgent { area_id, .. } => area_id,
        _ => return Err("invalid_request".into()),
    };
    let bound = areas
        .iter()
        .find(|a| a.area.id == *area_id)
        .ok_or("project_missing")?;
    let session = crate::workspace::desktop::mobile::session(database, &bound.area, cx)
        .ok_or("project_unavailable")?;
    session.update(cx, |app, cx| {
        let node = match change {
            WorkspaceConfigurationChange::AddAgent { definition_id, .. } => {
                let definition = app
                    .agent_definitions
                    .definitions()
                    .iter()
                    .find(|d| d.id == definition_id)
                    .cloned()
                    .ok_or("agent_missing")?;
                if !definition.enabled {
                    return Err("agent_disabled".into());
                }
                app.create_mobile_card_in_canvas(
                    definition.runtime_profile,
                    Some(definition.name),
                    Some(definition_id),
                    None,
                    cx,
                )
                .map_err(|e| e.to_string())?
            }
            WorkspaceConfigurationChange::RemoveAgent { node_id, .. } => {
                let snapshot = app
                    .mobile_card_snapshots()
                    .into_iter()
                    .find(|c| c.node_id == node_id)
                    .ok_or("target_missing")?;
                app.delete_mobile_card_in_canvas(&snapshot, false, None, cx)
                    .map_err(str::to_owned)?
            }
            _ => return Err("invalid_request".into()),
        };
        Ok(json!({"node_id":node}))
    })
}

pub(crate) fn fingerprint(projects: &Value) -> String {
    let mut values: Vec<_>=projects.as_array().into_iter().flatten().map(|p| {
        let p=if p.get("project").is_some(){&p["project"]}else{p};
        let mut cards:Vec<_>=p["agents"].as_array().into_iter().flatten().map(|a|json!({"definition":a["definition_id"],"node":a["card"]["node_id"],"name":a["name"],"profile":a["profile"]})).collect();
        cards.sort_by_key(Value::to_string);
        let mut definitions=p["definitions"].as_array().cloned().unwrap_or_default();
        definitions.sort_by_key(Value::to_string);
        json!({"area":p["area_id"],"cards":cards,"definitions":definitions})
    }).collect();
    values.sort_by_key(Value::to_string);
    json!(values).to_string()
}
