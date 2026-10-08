use super::*;
use norma::mobile::{activity_event, ActivityEvent, WorkspaceActivitySummary};
use sha2::{Digest, Sha256};
use std::path::Path;
use storage::{ActivityJournal, MobileActivityStore, PushEnvironment};
#[cfg(test)]
#[path = "../../../tests/unit/workspace_inbox_push_tests.rs"]
mod tests;

pub(super) fn tick(
    shared: &Shared,
    path: &Path,
    machine: &str,
    generation: u64,
    environment: PushEnvironment,
) -> Result<()> {
    let grants = shared.access()?.devices()?;
    let activities = MobileActivityStore::new(path.to_owned());
    let notifications = MobilePushStore::new(path.to_owned());
    let catalog = storage::DesktopWorkspaceStore::new(path.to_owned());
    let mut captured = HashMap::new();
    for grant in grants {
        let Some(mut state) = activities.get(&grant)? else {
            continue;
        };
        let ordinary_state = notifications.get(&grant)?;
        let ordinary = ordinary_state
            .as_ref()
            .and_then(|s| s.registration.as_ref())
            .filter(|r| r.environment == environment);
        let registration = state
            .registration
            .clone()
            .filter(|r| r.environment == environment);
        if registration.is_none() && ordinary.is_none() {
            continue;
        }
        let spaces = if ordinary.is_some()
            || registration
                .as_ref()
                .is_some_and(|r| r.desktop_workspace_id.is_none())
        {
            catalog
                .list()?
                .into_iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
        } else {
            registration
                .iter()
                .filter_map(|r| r.desktop_workspace_id.clone())
                .collect()
        };
        let mut all_facts = vec![];
        let mut all_areas = vec![];
        let mut complete = true;
        for workspace in spaces {
            let Ok(areas) =
                crate::mobile::activity::overview::areas(path, Some(&workspace), &grant, false)
            else {
                complete = false;
                continue;
            };
            if areas.is_empty() {
                continue;
            }
            let cache_key = serde_json::to_string(
                &areas
                    .iter()
                    .map(|a| {
                        (
                            &a.area.desktop_workspace_id,
                            &a.area.id,
                            &a.area.root,
                            &a.area.project_key,
                            &a.area.display_name,
                            &a.project_id,
                        )
                    })
                    .collect::<Vec<_>>(),
            )?;
            let facts = if let Some(value) = captured.get(&cache_key) {
                value
            } else {
                let Ok(value) = crate::mobile::activity::facts(shared, path, &workspace, &areas)
                else {
                    complete = false;
                    continue;
                };
                captured.entry(cache_key).or_insert(value)
            };
            let summary = WorkspaceActivitySummary::from_facts(machine, facts.clone(), now()?);
            all_facts.push(facts.clone());
            if registration.as_ref().is_some_and(|r| {
                r.desktop_workspace_id
                    .as_ref()
                    .is_none_or(|id| id == &workspace)
            }) {
                all_areas.extend(areas.clone());
            }
            if let Some(ordinary) = ordinary {
                for attention in &summary.attention {
                    let target = &attention.target;
                    let project_id = &areas
                        .iter()
                        .find(|a| a.area.id == target.area_id)
                        .ok_or_else(|| anyhow::anyhow!("activity area changed"))?
                        .project_id;
                    let identity = json!([
                        machine,
                        project_id,
                        attention.kind,
                        target.node_id,
                        target.profile,
                        target.run_id,
                        target.session_id,
                        target.turn_id,
                        target.request_id,
                        target.fingerprint
                    ])
                    .to_string();
                    let event_id = uuid_from(&identity);
                    if state.journal.alerted.contains(&event_id) {
                        continue;
                    }
                    if state.journal.alerted.len() >= 4096 {
                        tracing::warn!("workspace alert journal capacity reached");
                        continue;
                    }
                    state.journal.alerted.push(event_id.clone());
                    if !activities.update_journal(&grant, state.revision, &state.journal)? {
                        break;
                    }
                    let event = attention_event(
                        machine,
                        &workspace,
                        &ordinary.token,
                        &event_id,
                        now()?.saturating_add(300),
                        attention,
                    );
                    if activities.get(&grant)?.is_none()
                        || notifications
                            .get(&grant)?
                            .as_ref()
                            .and_then(|s| s.registration.as_ref())
                            != Some(ordinary)
                        || !crate::mobile::activity::overview::current(
                            path,
                            Some(&workspace),
                            &grant,
                            false,
                            &areas,
                        )
                    {
                        continue;
                    }
                    if let Some(invalidated) = send(shared, generation, &event)? {
                        let revision = ordinary_state
                            .as_ref()
                            .map(|s| s.revision)
                            .unwrap_or_default();
                        if invalidated == 0 {
                            notifications.replace(&grant, revision, None)?;
                        } else {
                            notifications.invalidate(&grant.device_id, revision, invalidated)?;
                        }
                    }
                }
            }
        }
        if !complete {
            continue;
        }
        let Some(registration) = registration.as_ref() else {
            continue;
        };
        let summary = if registration.desktop_workspace_id.is_some() {
            let Some(facts) = all_facts
                .into_iter()
                .find(|f| Some(&f.workspace.id) == registration.desktop_workspace_id.as_ref())
            else {
                continue;
            };
            WorkspaceActivitySummary::from_facts(machine, facts, now()?)
        } else {
            WorkspaceActivitySummary::from_workspaces(machine, all_facts, now()?)
        };
        if !summary.active
            && (registration.dismissed
                || (state.journal.ended_at.is_some() && !state.journal.quiet_seen))
        {
            activities.observe_idle(&grant, state.revision, now()?)?;
            continue;
        }
        if registration.dismissed {
            continue;
        }
        let previous = state
            .journal
            .summary
            .clone()
            .and_then(|v| serde_json::from_value::<WorkspaceActivitySummary>(v).ok());
        let event = activity_event(
            &summary,
            previous.as_ref(),
            state.journal.started_at,
            state.journal.ended_at,
            state.journal.last_sent_at,
            state.journal.quiet_seen,
        );
        let Some(event) = event else {
            continue;
        };
        let token = match event {
            ActivityEvent::Start if registration.auto_start => {
                registration.push_to_start_token.as_ref()
            }
            ActivityEvent::Start => None,
            _ => registration
                .activity_token
                .as_ref()
                .filter(|_| registration.cycle_id == state.journal.cycle_id),
        };
        // Start reserves its cycle even when APNs outcome is unknown. Updates can repair stale state.
        if event == ActivityEvent::Start && token.is_none() {
            continue;
        }
        let timestamp = now()?.max(state.journal.last_sent_at.saturating_add(1));
        let cycle = if event == ActivityEvent::Start {
            uuid::Uuid::new_v4().to_string()
        } else {
            state.journal.cycle_id.clone().unwrap_or_default()
        };
        let mut payload = json!({"kind":"activity","event":match event {ActivityEvent::Start=>"start",ActivityEvent::Update=>"update",ActivityEvent::End=>"end"},
                "event_id":uuid_from(&format!("{machine}:{cycle}:{timestamp}")),"token":token,"timestamp":timestamp,"stale_date":timestamp.saturating_add(120),
                "expires_at":timestamp.saturating_add(120),"priority":if event==ActivityEvent::Update && previous.as_ref().is_some_and(|p|p.attention==summary.attention){5}else{10},"content_state":content(&summary,timestamp)});
        if event == ActivityEvent::Start {
            payload["attributes"] = json!({"machine_id":machine,"desktop_workspace_id":registration.desktop_workspace_id,"generation":registration.generation,"cycle_id":cycle,"started_at":timestamp});
        }
        if event == ActivityEvent::End {
            payload["dismissal_date"] = json!(timestamp.saturating_add(600));
            payload["content_state"]["active"] = json!(false);
        }
        if serde_json::to_vec(&payload)?.len() > 4096 {
            tracing::warn!("workspace activity payload exceeds APNs budget");
            continue;
        }
        state.journal = ActivityJournal {
            cycle_id: Some(cycle),
            started_at: if event == ActivityEvent::Start {
                Some(timestamp)
            } else {
                state.journal.started_at
            },
            ended_at: if event == ActivityEvent::End {
                Some(timestamp)
            } else {
                None
            },
            last_sent_at: timestamp,
            quiet_seen: false,
            summary: Some(serde_json::to_value(&summary)?),
            alerted: state.journal.alerted.clone(),
        };
        if !activities.update_journal(&grant, state.revision, &state.journal)? {
            continue;
        }
        if let Some(token) = token {
            // Recheck both permission and token revision after capture and before HTTP.
            if activities.get(&grant)?.is_none_or(|s| {
                s.revision != state.revision || s.registration.as_ref() != Some(registration)
            }) || !crate::mobile::activity::overview::current(
                path,
                registration.desktop_workspace_id.as_deref(),
                &grant,
                false,
                &all_areas,
            ) {
                continue;
            }
            if send(shared, generation, &payload)?.is_some() {
                activities.invalidate(&grant, state.revision, token)?;
            }
        }
    }
    Ok(())
}
fn content(summary: &WorkspaceActivitySummary, timestamp: u64) -> Value {
    json!({"name":summary.name,"updated_at":timestamp,"active":summary.active,"running_count":summary.running_count,"approval_count":summary.approval_count,"review_count":summary.review_count,"failure_count":summary.failure_count,"hidden_projects":summary.hidden_projects,
        "projects":summary.projects.iter().map(|p|json!({"area_id":p.area_id,"desktop_workspace_id":p.desktop_workspace_id,"workspace_name":p.workspace_name,"name":p.name,"status":p.status,"reason":p.reason,"other_agents":p.other_agents,
            "agents":p.agents.iter().map(|a|json!({"name":a.name,"action":a.action})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
fn attention_event(
    machine: &str,
    workspace: &str,
    token: &str,
    event_id: &str,
    expires_at: u64,
    attention: &norma::mobile::WorkspaceActivityAttention,
) -> Value {
    let target = &attention.target;
    let mut event = json!({"kind":attention.kind,"event_id":event_id,"token":token,"expires_at":expires_at,
        "machine_id":machine,"workspace_id":crate::mobile::activity::scope(workspace,&target.area_id),"desktop_workspace_id":workspace,"area_id":target.area_id,
        "route_id":"workspace","node_id":target.node_id.to_string(),"run_id":target.run_id,"session_id":target.session_id,"turn_id":target.turn_id,
        "preview":{"project":attention.project,"card":attention.agent,"summary":attention.reason}});
    if let Some(request) = &target.request_id {
        event["request_id"] = json!(request);
    }
    if let Some(fingerprint) = &target.fingerprint {
        event["fingerprint"] = json!(fingerprint);
    }
    event
}
pub(super) fn uuid_from(identity: &str) -> String {
    let mut bytes: [u8; 16] = Sha256::digest(identity.as_bytes())[..16]
        .try_into()
        .expect("16 byte digest");
    bytes[6] = (bytes[6] & 15) | 80;
    bytes[8] = (bytes[8] & 63) | 128;
    uuid::Uuid::from_bytes(bytes).to_string()
}
fn send(shared: &Shared, generation: u64, event: &Value) -> Result<Option<u64>> {
    let delivery = shared
        .push_delivery
        .lock()
        .map_err(|_| anyhow::anyhow!("push unavailable"))?;
    if shared.stopped.load(Ordering::Acquire) || delivery.generation != generation {
        return Ok(None);
    }
    let Some(relay) = &delivery.relay else {
        return Ok(None);
    };
    match relay.send(event) {
        Ok(invalidated) => {
            tracing::info!(
                event_id = event["event_id"].as_str().unwrap_or_default(),
                kind = event["kind"].as_str().unwrap_or_default(),
                event = event["event"].as_str().unwrap_or_default(),
                accepted = invalidated.is_none(),
                "workspace push service response; device display unverified"
            );
            Ok(invalidated)
        }
        Err(code) => {
            tracing::warn!(code, "workspace push not confirmed");
            Ok(None)
        }
    }
}

/// The legacy alert path shares the same durable item ledger as workspace activities.
pub(super) fn reserve_alert(
    path: &Path,
    grant: &MobileDeviceGrant,
    event_id: &str,
) -> Result<bool> {
    let store = MobileActivityStore::new(path.to_owned());
    let Some(mut state) = store.get(grant)? else {
        return Ok(false);
    };
    if state.journal.alerted.iter().any(|id| id == event_id) || state.journal.alerted.len() >= 4096
    {
        return Ok(false);
    }
    state.journal.alerted.push(event_id.into());
    Ok(store.update_journal(grant, state.revision, &state.journal)?)
}
