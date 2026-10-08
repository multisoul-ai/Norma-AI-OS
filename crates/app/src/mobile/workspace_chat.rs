//! One authenticated business entry point for LAN and encrypted relay.
use super::server::Shared;
use norma::mobile::{WorkspaceChatMessage, WorkspaceConfigurationChange};
use serde_json::{json, Value};
use std::time::Duration;
use storage::{DesktopProjectArea, DesktopWorkspaceStore, MobileDeviceGrant, WorkspaceChatStore};

#[derive(Clone)]
pub struct WorkspaceArea {
    pub area: DesktopProjectArea,
    pub project_id: String,
}
pub enum LiveWorkspaceAction {
    ActivityCapture,
    ActivityDecision(super::activity::DecisionCall),
    Terminal {
        request: norma::mobile::WorkspaceTerminalRequest,
    },
    CreateAgent {
        device: String,
        token: String,
        request: norma::mobile::WorkspaceAgentCreateRequest,
    },
    Catalog,
    Message {
        device: String,
        token: String,
        message: WorkspaceChatMessage,
    },
    Agents {
        workspace: String,
        revision: u64,
        change: WorkspaceConfigurationChange,
    },
    Refresh,
    Recover {
        mac: String,
        workspace: String,
    },
}
pub struct LiveWorkspaceRequest {
    pub areas: Vec<WorkspaceArea>,
    pub action: LiveWorkspaceAction,
    pub reply: crossbeam_channel::Sender<Result<Value, String>>,
}

pub(super) fn handle(
    shared: &Shared,
    mac: &str,
    device: &str,
    token: &str,
    action: &super::server::connection::Action,
) -> Result<Value, &'static str> {
    use super::server::connection::Action;
    if let Action::WorkspaceActivity { request, .. } = action {
        return super::activity::handle(shared, mac, device, token, request);
    }
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(mac, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    let path = access.store.path().to_path_buf();
    let identities = access.store.workspace_identity_store();
    let catalog = DesktopWorkspaceStore::new(path.clone());
    let chat = WorkspaceChatStore::new(path);
    let build_areas =
        |workspace: &str, control: bool| -> Result<Vec<WorkspaceArea>, &'static str> {
            if catalog.get(workspace).map_err(|_| "unavailable")?.is_none() {
                return Err("workspace_missing");
            }
            let areas = catalog
                .list_projects(workspace)
                .map_err(|_| "unavailable")?;
            let mut result = Vec::new();
            for area in areas {
                let id = identities
                    .resolve_or_create(&area.root, &workspace::display_name_for_path(&area.root))
                    .map_err(|_| "configuration_changed")?;
                if !all_projects(&grant, control) && !grant.allows(&id.workspace_id, control) {
                    return Err("unauthorized");
                }
                result.push(WorkspaceArea {
                    area,
                    project_id: id.workspace_id,
                });
            }
            if result.is_empty() && !all_projects(&grant, control) {
                return Err("unauthorized");
            }
            Ok(result)
        };
    match action {
        Action::WorkspaceTerminal {
            request: terminal, ..
        } => {
            terminal.validate().map_err(|_| "invalid_request")?;
            let areas = build_areas(&terminal.desktop_workspace_id, false)?;
            let target_area = areas
                .iter()
                .find(|a| a.area.id == terminal.target.area_id)
                .ok_or("target_missing")?
                .clone();
            let revision = grant.revision;
            drop(access);
            let captured = request(
                shared,
                vec![target_area],
                LiveWorkspaceAction::Terminal {
                    request: (**terminal).clone(),
                },
            )?;
            // Recheck after the foreground capture: a revoke/permission change cannot leak one last frame.
            let access = shared.access().map_err(|_| "unavailable")?;
            if access
                .authenticate(mac, device, token)
                .map_err(|_| "unauthorized")?
                .is_none_or(|g| g.revision != revision)
            {
                return Err("unauthorized");
            }
            let mut captures = shared.terminal_captures.lock().map_err(|_| "unavailable")?;
            let update = if terminal.snapshot_id.is_some() {
                captures.continuation(device, terminal)?
            } else {
                captures.update(
                    device,
                    terminal,
                    serde_json::from_value(captured).map_err(|_| "unavailable")?,
                )?
            };
            Ok(
                json!({"status":"workspace_terminal","workspace_terminal":{"request":terminal,"update":update}}),
            )
        }
        Action::WorkspaceAgentCreate {
            request: creation, ..
        } => {
            creation.validate().map_err(|_| "invalid_request")?;
            let areas = build_areas(&creation.desktop_workspace_id, true)?;
            if !areas.iter().any(|a| a.area.id == creation.area_id) {
                return Err("project_missing");
            }
            let ledger = storage::AgentCreationStore::new(access.store.path().to_owned());
            let receipt = ledger
                .receive(
                    device,
                    mac,
                    &creation.desktop_workspace_id,
                    &creation.operation_id,
                    &serde_json::to_string(creation).map_err(|_| "invalid_request")?,
                )
                .map_err(store_error)?;
            drop(access);
            if receipt.state == "received" {
                let (reply, _) = crossbeam_channel::bounded(1);
                shared
                    .workspaces
                    .as_ref()
                    .ok_or("unavailable")?
                    .try_send(LiveWorkspaceRequest {
                        areas,
                        action: LiveWorkspaceAction::CreateAgent {
                            device: device.into(),
                            token: token.into(),
                            request: (**creation).clone(),
                        },
                        reply,
                    })
                    .map_err(|_| "busy")?;
            }
            Ok(
                json!({"status":"workspace_agent_creation_receipt","agent_creation_receipt":receipt}),
            )
        }
        Action::WorkspaceAgentCreationReceipt {
            desktop_workspace_id,
            operation_id,
            ..
        } => {
            let areas = build_areas(desktop_workspace_id, false)?;
            let ledger = storage::AgentCreationStore::new(access.store.path().to_owned());
            let mut receipt = ledger
                .receipt(device, mac, desktop_workspace_id, operation_id)
                .map_err(store_error)?;
            drop(access);
            if let Some(old) = receipt.clone() {
                if old.state == "outcome_unknown" {
                    if let (Some(binding), Ok(projects)) = (
                        &old.result,
                        request(shared, areas, LiveWorkspaceAction::Catalog),
                    ) {
                        for project in projects.as_array().into_iter().flatten() {
                            if project["project"]["area_id"] != binding["area_id"] {
                                continue;
                            }
                            for agent in project["project"]["agents"]
                                .as_array()
                                .into_iter()
                                .flatten()
                            {
                                if agent["definition_id"] != binding["definition_id"]
                                    || agent["card"]["node_id"] != binding["node_id"]
                                    || agent["name"] != binding["name"]
                                {
                                    continue;
                                }
                                if matches!(
                                    agent["card"]["state"].as_str(),
                                    Some("failed" | "ended")
                                ) {
                                    continue;
                                }
                                let verified = json!({"area_id":binding["area_id"],"definition_id":binding["definition_id"],"node_id":binding["node_id"],"name":agent["name"],"profile":agent["profile"],"card":agent["card"]});
                                ledger
                                    .resolve_unknown(device, operation_id, &verified)
                                    .map_err(store_error)?;
                                receipt = ledger
                                    .receipt(device, mac, desktop_workspace_id, operation_id)
                                    .map_err(store_error)?;
                                break;
                            }
                        }
                    }
                }
            }
            Ok(
                json!({"status":"workspace_agent_creation_receipt","agent_creation_receipt":receipt}),
            )
        }
        Action::WorkspaceCatalog { .. } => {
            let mut workspaces = Vec::new();
            let mut areas = Vec::new();
            for space in catalog.list().map_err(|_| "unavailable")? {
                let Ok(authorized) = build_areas(&space.id, false) else {
                    continue;
                };
                workspaces.push(json!({"id":space.id,"name":space.name,"revision":catalog.configuration_revision(&space.id).map_err(|_|"unavailable")?,"projects":[]}));
                areas.extend(authorized);
            }
            let available = identities.list().map_err(|_|"unavailable")?.into_iter().filter(|project|grant.allows(&project.workspace_id,true)).map(|project|json!({"project_id":project.workspace_id,"name":project.display_name,"root":project.canonical_root})).collect::<Vec<_>>();
            drop(access);
            let projects = request(shared, areas, LiveWorkspaceAction::Catalog)?;
            for space in &mut workspaces {
                let id = space["id"].as_str().unwrap_or_default().to_owned();
                space["projects"] = Value::Array(
                    projects
                        .as_array()
                        .ok_or("unavailable")?
                        .iter()
                        .filter(|p| p["desktop_workspace_id"] == id)
                        .map(|p| p["project"].clone())
                        .collect(),
                );
                let fingerprint =
                    super::workspace_live::configuration_fingerprint(&space["projects"]);
                space["revision"] = json!(catalog
                    .observe_agent_configuration(&id, &fingerprint)
                    .map_err(store_error)?);
            }
            Ok(
                json!({"status":"workspace_catalog","workspace_catalog":{"mac_id":mac,"workspaces":workspaces,"available_projects":available,"capabilities":["workspace_agent_create_v1","workspace_terminal_v1"]}}),
            )
        }
        Action::WorkspaceMessage { message, .. } => {
            message.validate().map_err(|_| "invalid_request")?;
            let areas = build_areas(&message.desktop_workspace_id, true)?;
            if message
                .target
                .as_ref()
                .is_some_and(|t| !areas.iter().any(|a| a.area.id == t.area_id))
            {
                return Err("target_missing");
            }
            let payload = serde_json::to_string(message).map_err(|_| "invalid_request")?;
            let receipt = chat
                .receive(
                    device,
                    mac,
                    &message.desktop_workspace_id,
                    &message.operation_id,
                    &payload,
                )
                .map_err(store_error)?;
            drop(access);
            if receipt.state == "received" {
                let (reply, _) = crossbeam_channel::bounded(1);
                shared
                    .workspaces
                    .as_ref()
                    .ok_or("unavailable")?
                    .try_send(LiveWorkspaceRequest {
                        areas,
                        action: LiveWorkspaceAction::Message {
                            device: device.into(),
                            token: token.into(),
                            message: (**message).clone(),
                        },
                        reply,
                    })
                    .map_err(|_| "busy")?;
            }
            Ok(json!({"status":"workspace_receipt","workspace_receipt":receipt}))
        }
        Action::WorkspaceReceipt {
            desktop_workspace_id,
            operation_id,
            ..
        } => {
            build_areas(desktop_workspace_id, false)?;
            let receipt = chat
                .receipt(device, mac, desktop_workspace_id, operation_id)
                .map_err(store_error)?;
            Ok(json!({"status":"workspace_receipt","workspace_receipt":receipt}))
        }
        Action::WorkspaceEvents {
            desktop_workspace_id,
            after,
            ..
        } => {
            let areas = build_areas(desktop_workspace_id, false)?;
            drop(access);
            request(
                shared,
                areas,
                LiveWorkspaceAction::Recover {
                    mac: mac.into(),
                    workspace: desktop_workspace_id.clone(),
                },
            )?;
            let events = chat
                .events(mac, desktop_workspace_id, *after)
                .map_err(store_error)?;
            Ok(json!({"status":"workspace_events","workspace_events":events}))
        }
        Action::WorkspaceConfigure {
            desktop_workspace_id,
            expected_revision,
            change,
            ..
        } => {
            if let WorkspaceConfigurationChange::Create { name } = change {
                if !all_projects(&grant, true) {
                    return Err("unauthorized");
                }
                let space = catalog.create(name).map_err(store_error)?;
                return Ok(
                    json!({"status":"workspace_configured","desktop_workspace_id":space.id}),
                );
            }
            let areas = build_areas(desktop_workspace_id, true)?;
            match change {
                WorkspaceConfigurationChange::Rename { name } => catalog
                    .rename_if_revision(desktop_workspace_id, *expected_revision, name)
                    .map_err(store_error)?,
                WorkspaceConfigurationChange::AddProject { project_id } => {
                    if !grant.allows(project_id, true) {
                        return Err("unauthorized");
                    }
                    let project = identities
                        .get(project_id)
                        .map_err(|_| "unavailable")?
                        .ok_or("project_missing")?;
                    catalog
                        .add_project_if_revision(
                            desktop_workspace_id,
                            *expected_revision,
                            &project.canonical_root,
                        )
                        .map_err(store_error)?;
                }
                WorkspaceConfigurationChange::RemoveProject { area_id } => catalog
                    .remove_project_if_revision(desktop_workspace_id, *expected_revision, area_id)
                    .map_err(store_error)?,
                WorkspaceConfigurationChange::AddAgent { .. }
                | WorkspaceConfigurationChange::RemoveAgent { .. } => {
                    drop(access);
                    request(
                        shared,
                        areas,
                        LiveWorkspaceAction::Agents {
                            workspace: desktop_workspace_id.clone(),
                            revision: *expected_revision,
                            change: change.clone(),
                        },
                    )?;
                    return Ok(json!({"status":"workspace_configured"}));
                }
                WorkspaceConfigurationChange::Create { .. } => unreachable!(),
            }
            drop(access);
            // The persistent change is already committed; a late UI refresh cannot turn it into a failed mutation.
            let _ = request(shared, areas, LiveWorkspaceAction::Refresh);
            Ok(json!({"status":"workspace_configured"}))
        }
        _ => Err("invalid_request"),
    }
}

pub(super) fn request(
    shared: &Shared,
    areas: Vec<WorkspaceArea>,
    action: LiveWorkspaceAction,
) -> Result<Value, &'static str> {
    let (reply, result) = crossbeam_channel::bounded(1);
    shared
        .workspaces
        .as_ref()
        .ok_or("unavailable")?
        .try_send(LiveWorkspaceRequest {
            areas,
            action,
            reply,
        })
        .map_err(|_| "busy")?;
    result
        .recv_timeout(Duration::from_secs(25))
        .map_err(|_| "unavailable")?
        .map_err(|error| match error.as_str() {
            "configuration_conflict" => "configuration_conflict",
            "state_not_ready" => "state_not_ready",
            "outcome_unknown" => "outcome_unknown",
            "configuration_changed" => "configuration_changed",
            "project_unavailable" => "project_unavailable",
            "project_missing" => "project_missing",
            "agent_missing" => "agent_missing",
            "agent_disabled" => "agent_disabled",
            "target_missing" => "target_missing",
            "session_changed" => "session_changed",
            "terminal_changed" => "terminal_changed",
            "terminal_unavailable" => "terminal_unavailable",
            "desktop_host_unavailable" => "desktop_host_unavailable",
            "unauthorized" => "unauthorized",
            _ => {
                tracing::warn!(%error,"mobile workspace request failed");
                "unavailable"
            }
        })
}
fn all_projects(grant: &MobileDeviceGrant, control: bool) -> bool {
    matches!(
        grant.all_projects_access,
        Some(storage::MobilePermission::Control)
    ) || (!control && grant.all_projects_access.is_some())
}
fn store_error(error: storage::StorageError) -> &'static str {
    match error {
        storage::StorageError::InvalidMobileCommand(code) => match code.as_str() {
            "configuration_conflict" => "configuration_conflict",
            "configuration_changed" => "configuration_changed",
            _ => "idempotency_conflict",
        },
        _ => "unavailable",
    }
}
