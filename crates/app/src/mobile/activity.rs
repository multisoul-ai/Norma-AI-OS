//! Authenticated workspace projection. All database reads and wire waits run off GPUI.
use super::{
    command::CommandLease,
    server::Shared,
    workspace_chat::{LiveWorkspaceAction, WorkspaceArea},
};
use norma::mobile::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};
use storage::{
    DesktopWorkspaceStore, MobileActivityRegistration, MobileActivityStore, MobileDeviceGrant,
};
mod detail;
#[cfg(test)]
#[path = "../../tests/unit/workspace_activity_api_tests.rs"]
mod tests;

#[derive(Serialize)]
pub struct Request {
    pub desktop_workspace_id: String,
    #[serde(flatten)]
    pub operation: Operation,
}
impl<'de> Deserialize<'de> for Request {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let mut fields = serde_json::Map::<String, Value>::deserialize(decoder)?;
        let workspace = fields
            .remove("desktop_workspace_id")
            .ok_or_else(|| serde::de::Error::missing_field("desktop_workspace_id"))?;
        let desktop_workspace_id =
            serde_json::from_value(workspace).map_err(serde::de::Error::custom)?;
        let operation: Operation = serde_json::from_value(Value::Object(fields.clone()))
            .map_err(serde::de::Error::custom)?;
        let allowed: &[&str] = match &operation {
            Operation::Summary | Operation::Subscription => &["kind"],
            Operation::SetSubscription { .. } => &["kind", "expected_revision", "registration"],
            Operation::Detail { .. } => &["kind", "target", "offset"],
            Operation::Decide { .. } => &["kind", "target", "operation_id", "decision"],
            Operation::Accept { .. } => &["kind", "target", "operation_id"],
            Operation::Receipt { .. } => &["kind", "area_id", "operation_id"],
        };
        if fields.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err(serde::de::Error::custom("unknown activity field"));
        }
        Ok(Self {
            desktop_workspace_id,
            operation,
        })
    }
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Summary,
    Subscription,
    SetSubscription {
        expected_revision: u64,
        registration: Option<MobileActivityRegistration>,
    },
    Detail {
        target: WorkspaceActivityTarget,
        #[serde(default)]
        offset: usize,
    },
    Decide {
        target: WorkspaceActivityTarget,
        operation_id: String,
        decision: norma::agent_hook::ApprovalDecision,
    },
    Accept {
        target: WorkspaceActivityTarget,
        operation_id: String,
    },
    Receipt {
        area_id: String,
        operation_id: String,
    },
}
pub struct DecisionCall {
    pub target: WorkspaceActivityTarget,
    pub decision: norma::agent_hook::ApprovalDecision,
    pub source: storage::NewMobileCommand,
    pub device: String,
    pub token: String,
    pub grant_revision: u64,
    pub lease: Arc<CommandLease>,
}
pub(super) fn areas(
    path: &Path,
    workspace: &str,
    grant: &MobileDeviceGrant,
    control: bool,
) -> Result<Vec<WorkspaceArea>, &'static str> {
    let app = storage::AppStore::new(path.to_owned());
    let catalog = DesktopWorkspaceStore::new(path.to_owned());
    if catalog.get(workspace).map_err(|_| "unavailable")?.is_none() {
        return Err("workspace_missing");
    }
    let mut result = Vec::new();
    for area in catalog
        .list_projects(workspace)
        .map_err(|_| "unavailable")?
    {
        let id = app
            .workspace_identity_store()
            .resolve_or_create(&area.root, &area.display_name)
            .map_err(|_| "configuration_changed")?;
        if !grant.allows(&id.workspace_id, control) {
            return Err("unauthorized");
        }
        result.push(WorkspaceArea {
            area,
            project_id: id.workspace_id,
        });
    }
    if result.is_empty()
        && !matches!(
            grant.all_projects_access,
            Some(storage::MobilePermission::Control)
        )
        && (control || grant.all_projects_access.is_none())
    {
        return Err("unauthorized");
    }
    Ok(result)
}
pub(super) fn handle(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    request: &Request,
) -> Result<Value, &'static str> {
    if !super::command::valid_id(&request.desktop_workspace_id) {
        return Err("invalid_request");
    }
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    let path = access.store.path().to_owned();
    let control = matches!(
        request.operation,
        Operation::Decide { .. } | Operation::Accept { .. }
    );
    let authorized = areas(&path, &request.desktop_workspace_id, &grant, control)?;
    drop(access);
    let store = MobileActivityStore::new(path.clone());
    let result = match &request.operation {
        Operation::Summary => {
            json!({"summary":summary(shared,&path,machine,&request.desktop_workspace_id,&authorized)?})
        }
        Operation::Subscription => {
            json!({"subscription":store.get(&grant).map_err(|_|"unavailable")?.ok_or("unauthorized")?})
        }
        Operation::SetSubscription {
            expected_revision,
            registration,
        } => {
            if registration
                .as_ref()
                .is_some_and(|r| r.desktop_workspace_id != request.desktop_workspace_id)
            {
                return Err("invalid_request");
            }
            let next = store
                .replace(&grant, *expected_revision, registration.as_ref())
                .map_err(|_| "invalid_request")?
                .ok_or("subscription_changed")?;
            json!({"subscription":next})
        }
        _ => detail::handle(shared, &path, device, token, &grant, request, &authorized)?,
    };
    // Revoke and catalog edits during a foreground handoff cannot leak a last result.
    let access = shared.access().map_err(|_| "unavailable")?;
    if access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .is_none_or(|g| g.revision != grant.revision)
        || !DesktopWorkspaceStore::new(path)
            .mobile_scope_current(
                &request.desktop_workspace_id,
                &authorized
                    .iter()
                    .map(|a| a.area.clone())
                    .collect::<Vec<_>>(),
            )
            .unwrap_or(false)
    {
        return Err("unauthorized");
    }
    Ok(json!({"status":"workspace_activity","workspace_activity":result}))
}
pub(super) fn capture(
    shared: &Shared,
    authorized: &[WorkspaceArea],
) -> Result<Value, &'static str> {
    super::workspace_chat::request(
        shared,
        authorized.to_vec(),
        LiveWorkspaceAction::ActivityCapture,
    )
}
pub(super) fn summary(
    shared: &Shared,
    path: &Path,
    machine: &str,
    workspace: &str,
    authorized: &[WorkspaceArea],
) -> Result<WorkspaceActivitySummary, &'static str> {
    let value = capture(shared, authorized)?;
    let catalog = DesktopWorkspaceStore::new(path.to_owned());
    let space = catalog
        .get(workspace)
        .map_err(|_| "unavailable")?
        .ok_or("workspace_missing")?;
    let mut projects = Vec::new();
    let mut approvals = Vec::new();
    let mut reviews = Vec::new();
    let deliveries = storage::MobileTaskDeliveryStore::new(path.to_owned());
    for entry in value.as_array().ok_or("unavailable")? {
        let mut project: DesktopChatProject =
            serde_json::from_value(entry["project"].clone()).map_err(|_| "unavailable")?;
        for approval in entry["approvals"].as_array().into_iter().flatten() {
            approvals.push(ActivityApprovalFact {
                area_id: project.area_id.clone(),
                approval: serde_json::from_value(approval.clone()).map_err(|_| "unavailable")?,
            });
        }
        let scope = scope(workspace, &project.area_id);
        let mut before = None;
        loop {
            let (page, _) = deliveries
                .page(&scope, None, before, true)
                .map_err(|_| "unavailable")?;
            for row in &page {
                let k = &row.outcome.key;
                if !project
                    .agents
                    .iter()
                    .any(|a| a.card.node_id == k.node_id && a.card.profile == k.profile)
                {
                    let bound = authorized
                        .iter()
                        .find(|a| a.area.id == project.area_id)
                        .ok_or("configuration_changed")?;
                    let members =
                        storage::ProjectStore::desktop_activity_members(path, &bound.area)
                            .map_err(|_| "unavailable")?;
                    let member = members.iter().find(|(n, d)| {
                        n.id == k.node_id && d.runtime_profile.display_title() == k.profile
                    });
                    let name = member
                        .map(|(_, d)| d.name.clone())
                        .unwrap_or_else(|| format!("已移除 Agent（卡片{}）", k.node_id));
                    let card:CardSnapshot=serde_json::from_value(json!({"node_id":k.node_id,"profile":k.profile,"title":name,"state":"delivered","mobile_control":false,"run_id":k.run_id,"session_id":k.session_id,"turn_id":k.turn_id})).map_err(|_|"unavailable")?;
                    project.agents.push(DesktopChatAgent {
                        definition_id: member.map(|(_, d)| d.id).unwrap_or(0),
                        name,
                        profile: k.profile.clone(),
                        card,
                    });
                }
                reviews.push(ActivityReviewFact {
                    area_id: project.area_id.clone(),
                    node_id: k.node_id,
                    profile: k.profile.clone(),
                    run_id: k.run_id.clone(),
                    session_id: k.session_id.clone(),
                    turn_id: k.turn_id.clone(),
                    fingerprint: row.fingerprint.clone(),
                    completed: true,
                    available: true,
                    conflicted: false,
                    accepted: false,
                    file_count: row.file_count,
                });
            }
            if page.len() < 20 {
                break;
            }
            before = page.last().map(|p| p.sequence);
            if reviews.len() > 4096 {
                return Err("activity_too_large");
            }
        }
        projects.push(project);
    }
    Ok(WorkspaceActivitySummary::from_facts(
        machine,
        WorkspaceActivityFacts {
            workspace: DesktopChatWorkspace {
                id: space.id,
                name: space.name,
                revision: catalog
                    .configuration_revision(workspace)
                    .map_err(|_| "unavailable")?,
                projects,
            },
            observed: true,
            approvals,
            reviews,
        },
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "unavailable")?
            .as_secs(),
    ))
}
pub(super) fn scope(workspace: &str, area: &str) -> String {
    format!("desktop:{workspace}:{area}")
}
fn key(target: &WorkspaceActivityTarget) -> storage::MobileTurnKey {
    storage::MobileTurnKey {
        node_id: target.node_id,
        profile: target.profile.clone(),
        run_id: target.run_id.clone(),
        session_id: target.session_id.clone(),
        turn_id: target.turn_id.clone(),
    }
}
fn hash(request: &Request) -> Result<String, &'static str> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(request).map_err(|_| "invalid_request")?,
    )))
}
