//! Exact-area quick creation. Durable claim precedes every side effect; no task is submitted.
use super::*;
use crate::workspace::persistence::PersistenceSaveState;
use storage::{AgentCreationStore, AgentDefinition, AgentDefinitionStore};

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
        cleanup(&session, None, &definition, &store, cx).await;
        return Err("outcome_unknown".into());
    }
    let prepared = cx
        .update(|cx| {
            if crate::workspace::desktop::mobile::session(database, &area, cx).as_ref()
                != Some(&session)
            {
                return Err("configuration_changed".to_owned());
            }
            session.update(cx, |app, cx| {
                if app.runtime.workspace_root() != Some(area.root.as_path()) {
                    return Err("configuration_changed".to_owned());
                }
                app.agent_definitions
                    .install_created_definition(definition.clone());
                app.prepare_quick_mobile_card(&definition, cx)
                    .map_err(|_| "create_failed".to_owned())
            })
        })
        .map_err(|_| "project_unavailable".to_owned())
        .and_then(|r| r);
    let (node, launch) = match prepared {
        Ok(value) => value,
        Err(error) => {
            cleanup(&session, None, &definition, &store, cx).await;
            return Err(error.to_string());
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
        cleanup(&session, Some(node), &definition, &store, cx).await;
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
            cleanup(&session, Some(node), &definition, &store, cx).await;
            return Err(code);
        }
    };
    let live = prepared.into_live();
    let completed = session.update(cx, |app, cx| {
        let result = app.runtime.complete_native_agent_terminal(node, &run, live);
        app.mark_canvas_dirty();
        app.mark_session_dirty();
        cx.notify();
        result
    });
    if !matches!(completed, Ok(Ok(()))) {
        cleanup(&session, Some(node), &definition, &store, cx).await;
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
        cleanup(&session, Some(node), &definition, &store, cx).await;
        return Err("agent_start_failed".into());
    }
    let member = json!({"area_id":area.id,"definition_id":definition.id,"name":definition.name,"profile":request.profile,"card":card});
    let record = ledger.clone();
    let phone = device.to_owned();
    let operation = request.operation_id.clone();
    cx.background_executor()
        .spawn(async move { record.bind(&phone, &operation, &member) })
        .await
        .map_err(|_| "outcome_unknown".to_owned())?;
    Ok(())
}

async fn cleanup(
    session: &Entity<MultisoulApp>,
    node: Option<u64>,
    definition: &AgentDefinition,
    store: &AgentDefinitionStore,
    cx: &mut gpui::AsyncApp,
) {
    let id = definition.id;
    if let Some(node) = node {
        let _=session.update(cx,|app,cx| {
            if matches!(app.runtime.document.node(node).map(|n|&n.kind),Some(canvas::CanvasNodeKind::Agent(s)) if s.definition_id==Some(id)) {
                app.runtime.remove_node(node);app.mark_canvas_dirty();app.mark_session_dirty();cx.notify();
            }
        });
    }
    let store = store.clone();
    if cx
        .background_executor()
        .spawn(async move { store.delete_if_quick(id) })
        .await
        .is_ok()
    {
        let _ = session.update(cx, |app, cx| {
            app.agent_definitions.forget_created_definition(id);
            cx.notify();
        });
    }
}
