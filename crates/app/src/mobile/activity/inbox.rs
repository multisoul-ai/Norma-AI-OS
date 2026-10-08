//! Inbox mutations share the durable command ledger and existing Provider delivery pipeline.
use super::super::command::{MobileCommand, MobileCommandRequest};
use super::*;
use std::sync::atomic::Ordering;
use storage::{MobileCommandReservation, MobileCommandStore, NewMobileCommand};

pub(super) fn validate(
    operation: &Operation,
) -> Result<(&WorkspaceActivityTarget, &str), &'static str> {
    let (target, operation_id) = match operation {
        Operation::Retry {
            target,
            operation_id,
        }
        | Operation::Dismiss {
            target,
            operation_id,
        }
        | Operation::ReturnForChanges {
            target,
            operation_id,
            ..
        } => (target, operation_id),
        _ => return Err("invalid_request"),
    };
    key(target).validate().map_err(|_| "invalid_request")?;
    if !super::super::command::valid_id(operation_id) || target.request_id.is_some() {
        return Err("invalid_request");
    }
    match operation {
        Operation::Retry { .. } | Operation::Dismiss { .. } if target.fingerprint.is_some() => {
            return Err("invalid_request")
        }
        Operation::ReturnForChanges { text, .. } => {
            feedback(target, text)?
                .validate()
                .map_err(|_| "invalid_request")?;
        }
        _ => {}
    }
    Ok((target, operation_id))
}
fn feedback(
    target: &WorkspaceActivityTarget,
    text: &str,
) -> Result<storage::DeliveryFeedback, &'static str> {
    Ok(storage::DeliveryFeedback {
        key: key(target),
        fingerprint: target.fingerprint.clone().ok_or("invalid_request")?,
        text: text.into(),
    })
}
pub(super) fn failed_card(
    project: &DesktopChatProject,
    target: &WorkspaceActivityTarget,
) -> Result<CardSnapshot, &'static str> {
    project
        .agents
        .iter()
        .map(|a| &a.card)
        .find(|card| {
            card.node_id == target.node_id
                && card.profile == target.profile
                && card.state == "failed"
                && card.run_id.as_ref() == Some(&target.run_id)
                && card.session_id.as_ref() == Some(&target.session_id)
                && card.turn_id.as_ref() == Some(&target.turn_id)
        })
        .cloned()
        .ok_or("target_missing")
}
pub(super) fn handle(
    shared: &Shared,
    path: &Path,
    device: &str,
    token: &str,
    grant: &MobileDeviceGrant,
    request: &Request,
    authorized: &[WorkspaceArea],
) -> Result<Value, &'static str> {
    let (target, operation) = validate(&request.operation)?;
    let area = authorized
        .iter()
        .find(|a| a.area.id == target.area_id)
        .ok_or("target_missing")?;
    let commands = MobileCommandStore::new(path.to_owned());
    let workspace = scope(&request.desktop_workspace_id, &target.area_id);
    // Use the original route for lookup: a restart/closed project cannot invalidate a completed receipt.
    let old = commands
        .get(device, operation, &workspace, &shared.owner_id)
        .map_err(|_| "unavailable")?;
    let mut source = NewMobileCommand {
        device_id: device.into(),
        operation_id: operation.into(),
        workspace_id: workspace,
        route_id: old
            .as_ref()
            .map(|r| r.route_id.clone())
            .unwrap_or_else(|| "closed".into()),
        owner_id: shared.owner_id.clone(),
        request_hash: hash(request)?,
    };
    if let Some(existing) = commands.lookup(&source).map_err(|_| "unavailable")? {
        return reservation(shared, existing);
    }
    let captured = capture(shared, authorized)?;
    let entry = captured
        .as_array()
        .ok_or("unavailable")?
        .iter()
        .find(|entry| entry["project"]["area_id"] == target.area_id)
        .ok_or("target_missing")?;
    let project: DesktopChatProject =
        serde_json::from_value(entry["project"].clone()).map_err(|_| "unavailable")?;
    source.route_id = entry["route_id"].as_str().unwrap_or("closed").into();
    let command = match &request.operation {
        Operation::Retry { .. } => Some(MobileCommand::RetryFailed {
            expected: failed_card(&project, target)?,
        }),
        Operation::Dismiss { .. } => {
            failed_card(&project, target)?;
            None
        }
        Operation::ReturnForChanges { text, .. } => {
            let saved = storage::MobileTaskDeliveryStore::new(path.to_owned())
                .get(&source.workspace_id, &key(target))
                .map_err(|_| "unavailable")?
                .filter(|d| Some(&d.fingerprint) == target.fingerprint.as_ref())
                .ok_or("delivery_changed")?;
            if saved.outcome.state != storage::MobileTurnState::Completed
                || !matches!(
                    saved.content,
                    storage::TaskDeliveryContent::Available { .. }
                )
            {
                return Err("delivery_not_ready");
            }
            if commands
                .delivery_acceptance(&source.workspace_id, &key(target))
                .map_err(|_| "unavailable")?
                .is_some()
            {
                return Err("delivery_accepted");
            }
            let expected = project
                .agents
                .iter()
                .map(|a| &a.card)
                .find(|card| {
                    card.node_id == target.node_id
                        && card.profile == target.profile
                        && card.run_id.as_ref() == Some(&target.run_id)
                        && card.session_id.as_ref() == Some(&target.session_id)
                })
                .cloned()
                .ok_or("target_missing")?;
            Some(MobileCommand::FeedbackDelivery {
                expected,
                feedback: feedback(target, text)?,
            })
        }
        _ => return Err("invalid_request"),
    };
    if command.is_some() && shared.commands.is_none() {
        return Err("unavailable");
    }
    let lease = shared.command_lease(device, operation)?;
    match commands.reserve(&source).map_err(|_| "unavailable")? {
        MobileCommandReservation::New => {}
        old => {
            lease.finished.store(true, Ordering::Release);
            return reservation(shared, old);
        }
    }
    let authorized_now = (|| {
        let access = shared.access().map_err(|_| "unavailable")?;
        let current = access
            .authenticate(&access.machine_id(), device, token)
            .map_err(|_| "unauthorized")?;
        if current.is_none_or(|g| g.revision != grant.revision)
            || !super::overview::current(
                path,
                Some(&request.desktop_workspace_id),
                grant,
                true,
                authorized,
            )
        {
            return Err("unauthorized");
        }
        Ok(())
    })();
    if let Err(code) = authorized_now {
        commands
            .finish(&source, Err(code.into()))
            .map_err(|_| "unavailable")?;
        lease.finished.store(true, Ordering::Release);
    } else if let Some(command) = command {
        let request = MobileCommandRequest {
            operation_id: operation.into(),
            workspace_id: source.workspace_id.clone(),
            route_id: source.route_id.clone(),
            grant_revision: grant.revision,
            command,
        };
        return super::super::command::dispatch_reserved(shared, request, source, commands, lease)
            .map(|value| json!({"receipt":value["receipt"]}));
    } else {
        let result = if lease.begin() {
            commands
                .dismiss_inbox_item(
                    &source,
                    &area.project_id,
                    storage::InboxItemKind::Failed,
                    &inbox_item_key("failed", &area.project_id, target),
                )
                .map(|saved| saved.result())
                .map_err(|_| "unavailable".into())
        } else {
            Err("unauthorized".into())
        };
        commands
            .finish(&source, result)
            .map_err(|_| "unavailable")?;
        lease.finished.store(true, Ordering::Release);
    }
    let receipt = commands
        .get(device, operation, &source.workspace_id, &shared.owner_id)
        .map_err(|_| "unavailable")?
        .ok_or("unavailable")?;
    super::super::project_events::changed();
    Ok(json!({"receipt":super::super::command::receipt_response(shared, receipt)["receipt"]}))
}
fn reservation(shared: &Shared, existing: MobileCommandReservation) -> Result<Value, &'static str> {
    match existing {
        MobileCommandReservation::Existing(receipt) => Ok(
            json!({"receipt":super::super::command::receipt_response(shared, *receipt)["receipt"]}),
        ),
        MobileCommandReservation::Conflict => Err("idempotency_conflict"),
        MobileCommandReservation::New => Err("unavailable"),
    }
}
