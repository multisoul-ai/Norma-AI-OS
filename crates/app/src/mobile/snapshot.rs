use super::{server::Shared, MobileAccess};
use anyhow::{ensure, Result};
use norma::mobile::ProjectSnapshot;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use storage::MobileDeviceGrant;

/// Bounded in-process request to the GPUI owner. Network callers never supply these project records.
pub struct LiveSnapshotRequest {
    pub projects: Vec<ProjectSnapshot>,
    pub include_cards: bool,
    /// Optional single-project roster, including definitions without an open card.
    pub agents: Option<crossbeam_channel::Sender<Vec<agent::AgentRosterEntry>>>,
    pub deadline: Instant,
    pub reply: crossbeam_channel::Sender<Vec<ProjectSnapshot>>,
}

impl MobileAccess {
    fn snapshot_projects(
        &self,
        grant: &MobileDeviceGrant,
        workspace: Option<&str>,
    ) -> Result<Vec<ProjectSnapshot>> {
        if let Some(id) = workspace {
            ensure!(grant.allows(id, false), "project not authorized");
        }
        let store = self.store.workspace_identity_store();
        grant
            .projects
            .iter()
            .filter(|scope| workspace.is_none_or(|id| id == scope.workspace_id))
            .map(|scope| {
                let project = store
                    .get(&scope.workspace_id)?
                    .ok_or_else(|| anyhow::anyhow!("project unavailable"))?;
                Ok(ProjectSnapshot {
                    workspace_id: project.workspace_id,
                    display_name: project.display_name.chars().take(256).collect(),
                    state: "unavailable".into(),
                    route_id: None,
                    card_count: None,
                    cards: None,
                })
            })
            .collect()
    }
}

pub(super) fn read(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    workspace: Option<&str>,
) -> Result<Value, &'static str> {
    read_until(
        shared,
        machine,
        device,
        token,
        workspace,
        Instant::now() + Duration::from_secs(2),
    )
}

pub(super) fn read_until(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    workspace: Option<&str>,
    deadline: Instant,
) -> Result<Value, &'static str> {
    let deadline = deadline.min(Instant::now() + Duration::from_secs(2));
    if Instant::now() >= deadline {
        return Err("unavailable");
    }
    let (grant, projects) = {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(machine, device, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if workspace.is_some_and(|id| !grant.allows(id, false)) {
            return Err("unauthorized");
        }
        let projects = access
            .snapshot_projects(&grant, workspace)
            .map_err(|_| "unavailable")?;
        (grant, projects)
    };
    let source = shared.snapshots.as_ref().ok_or("unavailable")?;
    let (reply, result) = crossbeam_channel::bounded(1);
    let (agents, roster) = crossbeam_channel::bounded(1);
    // 2s bounds foreground delivery, leaving 3s of the iOS 5s exchange budget for TLS/SQLite/wire.
    source
        .try_send(LiveSnapshotRequest {
            projects,
            include_cards: workspace.is_some(),
            agents: workspace.map(|_| agents),
            deadline,
            reply,
        })
        .map_err(|_| "unavailable")?;
    // Never hold the authorization mutex while waiting on the UI, so revoke/close cannot be blocked.
    let projects = result
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "unavailable")?;
    let access = shared.access().map_err(|_| "unavailable")?;
    let current = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if current.revision != grant.revision {
        return Err("unauthorized");
    }
    if projects.iter().any(|p| {
        !current.allows(&p.workspace_id, false) || workspace.is_some_and(|id| id != p.workspace_id)
    }) {
        return Err("unauthorized");
    }
    let agents =
        roster.try_recv().ok().map(|entries| {
            entries.into_iter().map(|entry| {
        json!({"definition_id":entry.definition_id,"name":entry.name,"speech_alias":entry.speech_alias,"profile":entry.profile,
            "enabled":entry.enabled,"active_node_id":entry.active_node_id,"busy":entry.busy})
    }).collect::<Vec<_>>()
        });
    let result = json!({"status":"snapshot","grant":current,"projects":projects,"agents":agents});
    // Fail explicitly rather than silently drop cards from a supposedly complete snapshot.
    if result.to_string().len() > 500 * 1024 {
        return Err("snapshot_too_large");
    }
    Ok(result)
}
