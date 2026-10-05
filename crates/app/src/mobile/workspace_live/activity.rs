use super::*;
use crate::mobile::activity::DecisionCall;

pub(super) fn start(database: PathBuf, call: LiveWorkspaceRequest, cx: &mut App) {
    match call.action {
        LiveWorkspaceAction::ActivityCapture => {
            let value = capture(&database, &call.areas, cx);
            cx.background_executor()
                .spawn(async move {
                    let _ = call.reply.try_send(value);
                })
                .detach();
        }
        LiveWorkspaceAction::ActivityDecision(decision) => {
            decide(database, call.areas, decision, call.reply, cx)
        }
        _ => unreachable!("activity bridge"),
    }
}
fn capture(database: &Path, areas: &[WorkspaceArea], cx: &mut App) -> Result<Value, String> {
    let mut entries = Vec::new();
    for bound in areas {
        let mut project = DesktopChatProject {
            area_id: bound.area.id.clone(),
            project_id: bound.project_id.clone(),
            name: bound.area.display_name.clone(),
            root: String::new(),
            agents: Vec::new(),
            definitions: Vec::new(),
        };
        let mut approvals = Vec::new();
        let mut route = None;
        if let Some(session) = crate::workspace::desktop::mobile::session(database, &bound.area, cx)
        {
            session.update(cx, |app, _| {
                if app.workspace_id.as_deref()
                    != Some(&crate::mobile::activity::scope(
                        &bound.area.desktop_workspace_id,
                        &bound.area.id,
                    ))
                {
                    return Err("configuration_changed".into());
                }
                route = app.workspace_route_id.clone();
                for card in app.mobile_card_snapshots() {
                    let Some(node) = app.runtime.document.node(card.node_id) else {
                        continue;
                    };
                    let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                        continue;
                    };
                    let Some(definition) = app
                        .agent_definitions
                        .definitions()
                        .iter()
                        .find(|d| Some(d.id) == spec.definition_id)
                    else {
                        continue;
                    };
                    approvals.extend(
                        app.runtime
                            .mobile_approval_snapshots(card.node_id)
                            .map_err(str::to_owned)?,
                    );
                    project.agents.push(DesktopChatAgent {
                        definition_id: definition.id,
                        name: definition.name.clone(),
                        profile: card.profile.clone(),
                        card,
                    });
                }
                Ok::<_, String>(())
            })?;
        }
        entries.push(json!({"project":project,"approvals":approvals,"route_id":route}));
    }
    Ok(json!(entries))
}
fn decide(
    database: PathBuf,
    areas: Vec<WorkspaceArea>,
    call: DecisionCall,
    reply: crossbeam_channel::Sender<Result<Value, String>>,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        let path = database.clone();
        let auth_areas = areas.clone();
        let device = call.device.clone();
        let token = call.token.clone();
        let revision = call.grant_revision;
        let authorized = cx
            .background_executor()
            .spawn(async move {
                storage::AppStore::new(path.clone())
                    .mobile_access_store()
                    .authenticate(&device, &token)
                    .ok()
                    .flatten()
                    .is_some_and(|g| {
                        g.revision == revision
                            && auth_areas.iter().all(|a| g.allows(&a.project_id, true))
                    })
                    && auth_areas.first().is_some_and(|a| {
                        storage::DesktopWorkspaceStore::new(path)
                            .mobile_scope_current(
                                &a.area.desktop_workspace_id,
                                &auth_areas
                                    .iter()
                                    .map(|a| a.area.clone())
                                    .collect::<Vec<_>>(),
                            )
                            .unwrap_or(false)
                    })
            })
            .await;
        let result = if !authorized {
            Err("unauthorized".into())
        } else {
            let receiver = cx.update(|cx| {
                let bound = areas
                    .iter()
                    .find(|a| a.area.id == call.target.area_id)
                    .ok_or("target_missing")?;
                let session =
                    crate::workspace::desktop::mobile::session(&database, &bound.area, cx)
                        .ok_or("project_unavailable")?;
                session.update(cx, |app, cx| {
                    if app.workspace_id.as_ref() != Some(&call.source.workspace_id)
                        || app.workspace_route_id.as_ref() != Some(&call.source.route_id)
                    {
                        return Err("session_changed");
                    }
                    let expected = norma::mobile::ApprovalTarget {
                        node_id: call.target.node_id,
                        profile: call.target.profile.clone(),
                        run_id: call.target.run_id.clone(),
                        session_id: call.target.session_id.clone(),
                        turn_id: call.target.turn_id.clone(),
                        request_id: call.target.request_id.clone().ok_or("invalid_request")?,
                    };
                    let approvals = app.runtime.mobile_approval_snapshots(expected.node_id)?;
                    if !approvals
                        .iter()
                        .any(|a| a.target == expected && a.decisions.contains(&call.decision))
                    {
                        return Err("approval_changed");
                    }
                    if !call.lease.begin() {
                        return Err("expired");
                    }
                    crate::norma::endpoint::mobile_approval::choose(
                        app,
                        &expected,
                        call.decision,
                        cx,
                    )
                })
            });
            match receiver {
                Ok(Ok(r)) => r.recv().await.unwrap_or(Err("outcome_unknown".into())),
                Ok(Err(e)) => Err(e.into()),
                Err(_) => Err("outcome_unknown".into()),
            }
        };
        let _ = reply.try_send(result);
    })
    .detach();
}
