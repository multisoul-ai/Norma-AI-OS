//! Exact-area quick creation. Durable claim precedes every side effect; no task is submitted.
use super::*;
use crate::workspace::persistence::PersistenceSaveState;
use storage::{AgentCreationStore, AgentDefinition, AgentDefinitionStore};
mod cleanup;
use cleanup::cleanup;

#[cfg(test)]
#[path = "../../../tests/unit/workspace_agent_creation_tests.rs"]
mod tests;

pub(super) fn start(database: PathBuf, call: LiveWorkspaceRequest, cx: &mut App) {
    let LiveWorkspaceAction::CreateAgent {
        device,
        token,
        request,
    } = call.action
    else {
        return;
    };
    cx.spawn(async move |cx| {
        let ledger = AgentCreationStore::new(database.clone());
        let claim = ledger.clone();
        let claim_device = device.clone();
        let operation = request.operation_id.clone();
        let claimed = cx
            .background_executor()
            .spawn(async move { claim.claim(&claim_device, &operation) })
            .await;
        if !matches!(claimed, Ok(true)) {
            return;
        }
        let result = create(
            &database,
            &call.areas,
            &device,
            &token,
            &request,
            &ledger,
            cx,
        )
        .await;
        let state = match &result {
            Ok(()) => "succeeded",
            Err(code) if code == "outcome_unknown" => "outcome_unknown",
            Err(_) => "failed",
        };
        let error = result.err();
        let operation = request.operation_id.clone();
        let _ = cx
            .background_executor()
            .spawn(async move { ledger.finish(&device, &operation, state, error.as_deref()) })
            .await;
    })
    .detach();
}

fn check_scope(
    database: &Path,
    areas: &[WorkspaceArea],
    device: &str,
    token: &str,
    request: &norma::mobile::WorkspaceAgentCreateRequest,
    verify_revision: bool,
) -> Result<(), String> {
    let store = storage::DesktopWorkspaceStore::new(database.to_owned());
    if !store
        .mobile_scope_current(
            &request.desktop_workspace_id,
            &areas.iter().map(|a| a.area.clone()).collect::<Vec<_>>(),
        )
        .map_err(|_| "project_unavailable")?
    {
        return Err("configuration_changed".into());
    }
    if verify_revision
        && store
            .configuration_revision(&request.desktop_workspace_id)
            .map_err(|_| "workspace_missing")?
            != request.expected_revision
    {
        return Err("configuration_conflict".into());
    }
    let grant = storage::AppStore::new(database.to_owned())
        .mobile_access_store()
        .authenticate(device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if !areas.iter().all(|a| grant.allows(&a.project_id, true)) {
        return Err("unauthorized".into());
    }
    let area = areas
        .iter()
        .find(|a| a.area.id == request.area_id)
        .ok_or("project_missing")?;
    let identity =
        workspace::project::identify_project(&area.area.root).map_err(|_| "project_unavailable")?;
    if identity.root != area.area.root || identity.key != area.area.project_key {
        return Err("configuration_changed".into());
    }
    Ok(())
}

fn launch_checked<T>(
    database: &Path,
    areas: &[WorkspaceArea],
    device: &str,
    token: &str,
    request: &norma::mobile::WorkspaceAgentCreateRequest,
    spawn: impl FnOnce() -> Result<T, terminal::PtyError>,
) -> Result<T, String> {
    check_scope(database, areas, device, token, request, false)?;
    spawn().map_err(|_| "agent_start_failed".to_owned())
}

fn install_in_original_runtime(
    runtime: &mut runtime::Runtime,
    expected_instance: &str,
    snapshot: canvas::CanvasNodeSnapshot,
) -> bool {
    runtime.runtime_instance_id() == expected_instance
        && runtime.document.install_reserved_node(snapshot)
}

fn quick_definition(
    store: &AgentDefinitionStore,
    profile: canvas::AgentProfile,
) -> Result<AgentDefinition, String> {
    // The workspace queue has eight slots. Nine attempts cover one collision per concurrent creator.
    for _ in 0..9 {
        let definitions = store.list().map_err(|_| "definition_store_unavailable")?;
        let name =
            agent::AgentNameRegistry::from_names(definitions.iter().map(|d| d.name.as_str()))
                .map_err(|_| "name_unavailable")?
                .next_available_name()
                .ok_or("name_unavailable")?;
        match store.create_quick(&storage::NewAgentDefinition {
            name,
            runtime_profile: profile,
            workflow_description: String::new(),
        }) {
            Ok(definition) => {
                return Ok(definition);
            }
            Err(storage::StorageError::AgentNameConflict { .. }) => continue,
            Err(_) => return Err("definition_store_unavailable".into()),
        }
    }
    Err("name_unavailable".into())
}

async fn create(
    database: &Path,
    areas: &[WorkspaceArea],
    device: &str,
    token: &str,
    request: &norma::mobile::WorkspaceAgentCreateRequest,
    ledger: &AgentCreationStore,
    cx: &mut gpui::AsyncApp,
) -> Result<(), String> {
    let area = areas
        .iter()
        .find(|a| a.area.id == request.area_id)
        .ok_or("project_missing")?
        .area
        .clone();
    let load = cx
        .update(|cx| crate::workspace::desktop::mobile::ensure(database, area.clone(), cx))
        .map_err(|_| "desktop_host_unavailable")?;
    let session = load.await?;
    let runtime_instance = session
        .read_with(cx, |app, _| app.runtime.runtime_instance_id().to_owned())
        .map_err(|_| "project_unavailable")?;
    let store = session
        .read_with(cx, |app, _| app.agent_definitions.creation_store())
        .map_err(|_| "project_unavailable")?
        .ok_or("definition_store_unavailable")?;
    let profile = match request.profile.as_str() {
        "Codex" => canvas::AgentProfile::Codex,
        "Claude" => canvas::AgentProfile::Claude,
        "Cursor" => canvas::AgentProfile::Cursor,
        _ => return Err("unsupported_profile".into()),
    };
    let db = database.to_owned();
    let bounds = areas.to_vec();
    let phone = device.to_owned();
    let credential = token.to_owned();
    let original = request.clone();
    let definitions_store = store.clone();
    let definition = cx
        .background_executor()
        .spawn(async move {
            check_scope(&db, &bounds, &phone, &credential, &original, true)?;
            quick_definition(&definitions_store, profile)
        })
        .await?;
    let record = ledger.clone();
    let phone = device.to_owned();
    let operation = request.operation_id.clone();
    let id = definition.id;
    let name = definition.name.clone();
    let area_id = area.id.clone();
    let bound = cx
        .background_executor()
        .spawn(async move {
            record.bind(
                &phone,
                &operation,
                &json!({"definition_id":id,"name":name,"area_id":area_id}),
            )
        })
        .await;
    if bound.is_err() {
        cleanup(&session, &runtime_instance, None, &definition, &store, cx).await;
        return Err("outcome_unknown".into());
    }
    let reserved = cx
        .update(|cx| {
            if crate::workspace::desktop::mobile::session(database, &area, cx).as_ref()
                != Some(&session)
            {
                return Err("configuration_changed".to_owned());
            }
            session.update(cx, |app, _| {
                if app.runtime.runtime_instance_id() != runtime_instance
                    || app.runtime.workspace_root() != Some(area.root.as_path())
                {
                    return Err("configuration_changed".to_owned());
                }
                app.agent_definitions
                    .install_created_definition(definition.clone());
                let bounds = app.next_tiled_node_bounds_for_area(
                    app.mobile_canvas_area(None),
                    app.runtime.document.viewport.zoom,
                );
                let node = app.runtime.document.reserve_node(
                    canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
                        profile: definition.runtime_profile,
                        alias: Some(definition.name.clone()),
                        definition_id: Some(definition.id),
                        mobile_control: true,
                    }),
                    bounds,
                );
                let canvas = app
                    .persistence
                    .project_store()
                    .ok_or("project_unavailable")?
                    .canvas_store();
                let publication = app.persistence.begin_card_publication();
                Ok((node, canvas, publication))
            })
        })
        .map_err(|_| "project_unavailable".to_owned())??;
    let (mut snapshot, canvas, publication) = reserved;
    let card = agent::workspace_dispatch::WorkspaceCardRef {
        workspace_id: area.desktop_workspace_id.clone(),
        area_id: area.id.clone(),
        node_id: snapshot.id,
        card_instance_id: snapshot.card_instance_id.clone(),
    };
    let roles = storage::WorkspaceAgentRoleStore::new(database.to_owned());
    let db = database.to_owned();
    let bounds = areas.to_vec();
    let phone = device.to_owned();
    let credential = token.to_owned();
    let original = request.clone();
    let (materialized, absent) = cx
        .background_executor()
        .spawn(async move {
            let node_id = snapshot.id;
            let identity = snapshot.card_instance_id.clone();
            let result = (|| {
                check_scope(&db, &bounds, &phone, &credential, &original, false)?;
                roles
                    .register_workspace(&card.workspace_id)
                    .map_err(|_| "name_unavailable")?;
                let role = roles
                    .materialize_workspace_card(&card, &canvas, &snapshot, false)
                    .map_err(|_| "create_failed")?;
                if let canvas::CanvasNodeKind::Agent(spec) = &mut snapshot.kind {
                    spec.alias = Some(role.name);
                }
                Ok::<_, String>(snapshot)
            })();
            let absent = matches!(canvas.contains_card_instance(node_id, &identity), Ok(false));
            (result, absent)
        })
        .await;
    let role_name = materialized.as_ref().ok().and_then(|s| match &s.kind {
        canvas::CanvasNodeKind::Agent(spec) => spec.alias.clone(),
        _ => None,
    });
    let session_current = cx
        .update(|cx| {
            crate::workspace::desktop::mobile::session(database, &area, cx).as_ref()
                == Some(&session)
        })
        .unwrap_or(false);
    let prepared = session
        .update(cx, |app, cx| {
            let snapshot = match materialized {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    app.persistence
                        .finish_card_publication(&publication, false, absent);
                    return Err(error);
                }
            };
            let node = snapshot.id;
            let installed = session_current
                && app.workspace_role_scope()
                    == Some((area.desktop_workspace_id.as_str(), area.id.as_str()))
                && install_in_original_runtime(&mut app.runtime, &runtime_instance, snapshot);
            app.persistence
                .finish_card_publication(&publication, installed, absent);
            if !installed {
                return Err(if absent {
                    "configuration_changed"
                } else {
                    "outcome_unknown"
                }
                .into());
            }
            let launch = app
                .runtime
                .prepare_native_agent_terminal(node, 80, 24)
                .map_err(|_| "create_failed".to_owned())?;
            app.mark_canvas_dirty();
            app.mark_session_dirty();
            cx.notify();
            Ok((node, launch))
        })
        .map_err(|_| "project_unavailable".to_owned())?;
    let (node, launch) = match prepared {
        Ok(value) => value,
        Err(error) => {
            cleanup(&session, &runtime_instance, None, &definition, &store, cx).await;
            return Err(error);
        }
    };
    let run = launch.run_id().to_owned();
    let receipt_run = run.clone();
    let record = ledger.clone();
    let phone = device.to_owned();
    let operation = request.operation_id.clone();
    if cx
        .background_executor()
        .spawn(
            async move { record.bind(&phone, &operation, &json!({"node_id":node,"run_id":run})) },
        )
        .await
        .is_err()
    {
        cleanup(
            &session,
            &runtime_instance,
            Some(node),
            &definition,
            &store,
            cx,
        )
        .await;
        return Err("outcome_unknown".into());
    }
    let run = receipt_run;
    let db = database.to_owned();
    let bounds = areas.to_vec();
    let phone = device.to_owned();
    let credential = token.to_owned();
    let original = request.clone();
    let spawned = cx
        .background_executor()
        .spawn(async move {
            launch_checked(&db, &bounds, &phone, &credential, &original, || {
                launch.spawn()
            })
        })
        .await;
    let prepared = match spawned {
        Ok(prepared) => prepared,
        Err(code) => {
            cleanup(
                &session,
                &runtime_instance,
                Some(node),
                &definition,
                &store,
                cx,
            )
            .await;
            return Err(code);
        }
    };
    let live = prepared.into_live();
    let session_current = cx
        .update(|cx| {
            crate::workspace::desktop::mobile::session(database, &area, cx).as_ref()
                == Some(&session)
        })
        .unwrap_or(false);
    let completed = session.update(cx, |app, cx| {
        if !session_current || app.runtime.runtime_instance_id() != runtime_instance {
            return Err(terminal::PtyError::Spawn("configuration_changed".into()));
        }
        let result = app.runtime.complete_native_agent_terminal(node, &run, live);
        app.mark_canvas_dirty();
        app.mark_session_dirty();
        cx.notify();
        result
    });
    if !matches!(completed, Ok(Ok(()))) {
        cleanup(
            &session,
            &runtime_instance,
            Some(node),
            &definition,
            &store,
            cx,
        )
        .await;
        return Err("agent_start_failed".into());
    }
    // Normal desktop writer owns snapshots. 20 seconds covers its 360ms debounce and disk retries.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let saved = session
            .read_with(cx, |app, _| app.persistence.save_state())
            .map_err(|_| "outcome_unknown")?;
        match saved {
            PersistenceSaveState::Clean => break,
            PersistenceSaveState::Failed => return Err("outcome_unknown".into()),
            PersistenceSaveState::Pending => {}
        }
        if std::time::Instant::now() >= deadline {
            return Err("outcome_unknown".into());
        }
        // 50ms gives 20 inexpensive memory checks/second while waiting for the existing writer.
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
    }
    let card = session
        .read_with(cx, |app, _| {
            app.mobile_card_snapshots()
                .into_iter()
                .find(|c| c.node_id == node)
        })
        .map_err(|_| "outcome_unknown")?
        .ok_or("target_missing")?;
    if matches!(card.state.as_str(), "failed" | "ended") {
        cleanup(
            &session,
            &runtime_instance,
            Some(node),
            &definition,
            &store,
            cx,
        )
        .await;
        return Err("agent_start_failed".into());
    }
    let member = json!({"area_id":area.id,"definition_id":definition.id,"name":role_name,"profile":request.profile,"card":card});
    let record = ledger.clone();
    let phone = device.to_owned();
    let operation = request.operation_id.clone();
    cx.background_executor()
        .spawn(async move { record.bind(&phone, &operation, &member) })
        .await
        .map_err(|_| "outcome_unknown".to_owned())?;
    Ok(())
}
