//! One app-owned background consumer with bounded final-output previews.
use super::server::Shared;
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::Ordering, Arc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use storage::{
    MobileDeviceGrant, MobilePushState, MobilePushStore, MobileTurnKey, MobileTurnState,
    MobileTurnStore,
};

mod activity;
pub(crate) mod configuration;
mod preview;
mod relay;
mod source;
pub use relay::PushRelayConfig;
use relay::Relay;

#[derive(Default)]
pub(super) struct Delivery {
    generation: u64,
    relay: Option<Relay>,
}

struct Notice {
    workspace: String,
    route: String,
    key: MobileTurnKey,
    request: Option<String>,
    kind: &'static str,
    created: u64,
    summary: Option<String>,
}
impl Notice {
    fn event(&self, machine: &str, token: &str) -> Value {
        let identity = json!([
            machine,
            self.workspace,
            self.kind,
            self.key.node_id,
            self.key.profile,
            self.key.run_id,
            self.key.session_id,
            self.key.turn_id,
            self.request,
            Option::<String>::None
        ]);
        let event_id = activity::uuid_from(&identity.to_string());
        let mut value = json!({"event_id":event_id,"token":token,"expires_at":self.created.saturating_add(300),
            "kind":self.kind,"machine_id":machine,"workspace_id":self.workspace,"route_id":self.route,
            "node_id":self.key.node_id.to_string(),"run_id":self.key.run_id,"session_id":self.key.session_id,"turn_id":self.key.turn_id});
        if let Some(request) = &self.request {
            value["request_id"] = json!(request);
        }
        value
    }
}

pub(super) fn start(shared: Arc<Shared>, config: PushRelayConfig) -> Result<()> {
    configure(shared, Some(config))
}

pub(super) fn configure(shared: Arc<Shared>, config: Option<PushRelayConfig>) -> Result<()> {
    ensure!(
        shared.snapshots.is_some() && shared.approvals.is_some(),
        "Live data sources for mobile notifications are unavailable"
    );
    let mut delivery = shared
        .push_delivery
        .lock()
        .map_err(|_| anyhow::anyhow!("Notification service is unavailable"))?;
    ensure!(
        !shared.stopped.load(Ordering::Acquire),
        "Mobile connection is off"
    );
    // Serialize replacement with the last HTTP send (bounded at 12s). No Agent or WSS restart.
    delivery.generation += 1;
    delivery.relay = config.map(Relay::new);
    if delivery.relay.is_none() || shared.push_started.load(Ordering::Acquire) {
        return Ok(());
    }
    let worker = Arc::clone(&shared);
    shared.push_started.store(true, Ordering::Release);
    if let Err(error) = std::thread::Builder::new()
        .name("norma-mobile-push".into())
        .spawn(move || {
            if run(&worker).is_err() && !worker.stopped.load(Ordering::Acquire) {
                tracing::warn!("mobile push worker stopped; notification delivery unavailable");
            }
            worker.push_started.store(false, Ordering::Release);
        })
    {
        shared.push_started.store(false, Ordering::Release);
        return Err(error.into());
    }
    Ok(())
}

fn run(shared: &Shared) -> Result<()> {
    let (path, machine) = {
        let access = shared.access()?;
        (access.store.path().to_path_buf(), access.machine_id())
    };
    let store = MobilePushStore::new(path.clone());
    let mut cursors = HashMap::new();
    let mut attempted: HashSet<(String, String, String)> = HashSet::new();
    let mut generation = 0;
    while !shared.stopped.load(Ordering::Acquire) {
        let config = {
            let delivery = shared
                .push_delivery
                .lock()
                .map_err(|_| anyhow::anyhow!("Notification service is unavailable"))?;
            if generation != delivery.generation {
                cursors.clear(); // Establish fresh terminal watermarks; never replay a disabled interval.
                generation = delivery.generation;
            }
            delivery.relay.as_ref().map(|r| r.config.environment)
        };
        let Some(environment) = config else {
            // ponytail: one 1Hz idle worker after disabling; use a wake channel if power measurements require it.
            std::thread::sleep(Duration::from_secs(1));
            continue;
        };
        if let Err(error) = activity::tick(shared, &path, &machine, generation, environment) {
            tracing::warn!(%error,"workspace activity tick failed");
        }
        let subscribers = {
            let access = shared.access()?;
            access
                .devices()?
                .into_iter()
                .filter_map(|grant| match store.get(&grant) {
                    Ok(Some(state)) if state.registration.as_ref().is_some() => {
                        Some(Ok((grant, state)))
                    }
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let workspaces = subscribers
            .iter()
            .flat_map(|(grant, _)| {
                grant
                    .projects
                    .iter()
                    .map(|project| project.workspace_id.clone())
            })
            .collect::<HashSet<_>>();
        ensure!(
            workspaces.len() <= 256,
            "notification project capacity reached"
        );
        cursors.retain(|id, _| workspaces.contains(id));
        attempted.retain(|(workspace, device, _)| {
            subscribers
                .iter()
                .any(|(grant, _)| grant.device_id == *device && grant.allows(workspace, false))
        });
        for workspace in &workspaces {
            if !cursors.contains_key(workspace) {
                let turns = MobileTurnStore::for_workspace(path.clone(), workspace.clone());
                cursors.insert(workspace.clone(), turns.latest_sequence()?);
            }
        }
        if !workspaces.is_empty() {
            if let Some(projects) =
                source::snapshots(shared, &workspaces.into_iter().collect::<Vec<_>>())
            {
                for project in projects {
                    let Some(cursor) = cursors.get_mut(&project.workspace_id) else {
                        continue;
                    };
                    let Some(route) = project
                        .route_id
                        .as_ref()
                        .filter(|_| project.state == "open")
                    else {
                        continue;
                    };
                    let mut notices = Vec::new();
                    let approvals = source::approvals(shared, &project.workspace_id, route);
                    let approvals_current = approvals.is_some();
                    let mut pending_ids = HashSet::new();
                    if let Some(items) = approvals {
                        for item in items {
                            // External hooks have no phone decisions. Only actual actionable requests are alerts.
                            if !item.target.valid() || item.decisions.is_empty() {
                                continue;
                            }
                            let t = item.target;
                            notices.push(Notice {
                                workspace: project.workspace_id.clone(),
                                route: route.clone(),
                                key: MobileTurnKey {
                                    node_id: t.node_id,
                                    profile: t.profile,
                                    run_id: t.run_id,
                                    session_id: t.session_id,
                                    turn_id: t.turn_id,
                                },
                                request: Some(t.request_id),
                                kind: "approval",
                                created: now()?,
                                summary: None,
                            });
                        }
                    }
                    let turns =
                        MobileTurnStore::for_workspace(path.clone(), project.workspace_id.clone());
                    // At most 64 committed outcomes per project/tick; the next cursor page retains a burst.
                    for record in turns.after(*cursor, 64)? {
                        *cursor = record.sequence;
                        let kind = match record.outcome.state {
                            MobileTurnState::Completed => continue,
                            MobileTurnState::Failed => "failed",
                            MobileTurnState::Interrupted => continue,
                        };
                        notices.push(Notice {
                            workspace: project.workspace_id.clone(),
                            route: route.clone(),
                            key: record.outcome.key,
                            request: None,
                            kind,
                            created: record.recorded_at_unix,
                            summary: record.outcome.summary,
                        });
                    }
                    for notice in notices {
                        if notice.created > now()?
                            || notice.created.saturating_add(300) <= now()?
                            || !source::matches_card(&project, &notice)
                        {
                            continue;
                        }
                        let event_id = notice.event(&machine, "")["event_id"]
                            .as_str()
                            .expect("generated ID")
                            .to_owned();
                        if notice.request.is_some() {
                            pending_ids.insert(event_id.clone());
                        }
                        for (grant, subscription) in &subscribers {
                            if !grant.allows(&notice.workspace, false) {
                                continue;
                            }
                            let registration = subscription
                                .registration
                                .as_ref()
                                .expect("active subscriber");
                            if registration.environment != environment {
                                continue;
                            }
                            let mut event = notice.event(&machine, &registration.token);
                            event["preview"] = preview::content(&project, &notice);
                            let identity = (
                                notice.workspace.clone(),
                                grant.device_id.clone(),
                                event_id.clone(),
                            );
                            if attempted.contains(&identity) {
                                continue;
                            }
                            // ponytail: at most 4096 active approval/device pairs; resolved approvals are
                            // pruned after a successful source read. Terminal events advance a durable cursor.
                            ensure!(
                                attempted.len() < 4096,
                                "notification pending capacity reached"
                            );
                            if !source::still_current(shared, &notice) {
                                continue;
                            }
                            if !authorized(shared, &store, grant, subscription, &notice.workspace)?
                            {
                                continue;
                            }
                            if shared.stopped.load(Ordering::Acquire)
                                || notice.created.saturating_add(300) <= now()?
                            {
                                continue;
                            }
                            let delivery = shared.push_delivery.lock().map_err(|_| {
                                anyhow::anyhow!("Notification service is unavailable")
                            })?;
                            let Some(relay) = delivery
                                .relay
                                .as_ref()
                                .filter(|_| delivery.generation == generation)
                            else {
                                continue; // A source read begun before replacement cannot send with either credential.
                            };
                            if shared.stopped.load(Ordering::Acquire) {
                                continue;
                            }
                            if notice.request.is_some() {
                                attempted.insert(identity);
                            }
                            if !activity::reserve_alert(
                                &path,
                                grant,
                                event["event_id"]
                                    .as_str()
                                    .ok_or_else(|| anyhow::anyhow!("invalid alert identity"))?,
                            )? {
                                continue;
                            }
                            match relay.send(&event) {
                                Ok(Some(0)) => {
                                    store.replace(grant, subscription.revision, None)?;
                                }
                                Ok(Some(time)) => {
                                    store.invalidate(
                                        &grant.device_id,
                                        subscription.revision,
                                        time,
                                    )?;
                                }
                                Ok(None) => {} // APNs acceptance is not evidence that the phone displayed anything.
                                Err(code) => tracing::warn!(
                                    code,
                                    "mobile push attempt not confirmed; not replaying"
                                ),
                            }
                        }
                    }
                    if approvals_current {
                        attempted.retain(|(workspace, _, id)| {
                            *workspace != project.workspace_id || pending_ids.contains(id)
                        });
                    }
                }
            }
        }
        // 1Hz only while a configured app-owned service exists; no polling on GPUI's foreground.
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

fn authorized(
    shared: &Shared,
    store: &MobilePushStore,
    previous: &MobileDeviceGrant,
    subscription: &MobilePushState,
    workspace: &str,
) -> Result<bool> {
    let access = shared.access()?;
    let current = access.devices()?.into_iter().find(|grant| {
        grant.device_id == previous.device_id
            && grant.revision == previous.revision
            && grant.allows(workspace, false)
    });
    let Some(current) = current else {
        return Ok(false);
    };
    Ok(store.get(&current)?.as_ref() == Some(subscription))
}
fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
