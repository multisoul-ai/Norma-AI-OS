use super::*;
use std::sync::atomic::Ordering;
use storage::{MobileCommandReservation, MobileCommandStore, MobileTaskDeliveryStore};

pub(super) fn handle(
    shared: &Shared,
    path: &Path,
    device: &str,
    token: &str,
    grant: &MobileDeviceGrant,
    request: &Request,
    authorized: &[WorkspaceArea],
) -> Result<Value, &'static str> {
    let commands = MobileCommandStore::new(path.to_owned());
    if let Operation::Receipt {
        area_id,
        operation_id,
    } = &request.operation
    {
        if !authorized.iter().any(|a| a.area.id == *area_id)
            || !super::super::command::valid_id(operation_id)
        {
            return Err("invalid_request");
        }
        let receipt = commands
            .get(
                device,
                operation_id,
                &scope(&request.desktop_workspace_id, area_id),
                &shared.owner_id,
            )
            .map_err(|_| "unavailable")?
            .ok_or("not_found")?;
        return Ok(
            json!({"receipt":super::super::command::receipt_response(shared,receipt)["receipt"]}),
        );
    }
    let target = match &request.operation {
        Operation::Detail { target, .. }
        | Operation::Decide { target, .. }
        | Operation::Accept { target, .. } => target,
        _ => return Err("invalid_request"),
    };
    key(target).validate().map_err(|_| "invalid_request")?;
    if !authorized.iter().any(|a| a.area.id == target.area_id) {
        return Err("target_missing");
    }
    let workspace = scope(&request.desktop_workspace_id, &target.area_id);
    let captured = capture(shared, authorized)?;
    let entry = captured
        .as_array()
        .ok_or("unavailable")?
        .iter()
        .find(|p| p["project"]["area_id"] == target.area_id)
        .ok_or("target_missing")?;
    let route = entry["route_id"].as_str().unwrap_or("closed");
    let approvals: Vec<ApprovalSnapshot> =
        serde_json::from_value(entry["approvals"].clone()).map_err(|_| "unavailable")?;
    let approval = approvals.iter().find(|a| {
        let t = &a.target;
        t.node_id == target.node_id
            && t.profile == target.profile
            && t.run_id == target.run_id
            && t.session_id == target.session_id
            && t.turn_id == target.turn_id
            && Some(&t.request_id) == target.request_id.as_ref()
    });
    if let Operation::Detail { offset, .. } = &request.operation {
        if target.request_id.is_some() {
            return Ok(json!({"target":target,"approval":approval.ok_or("approval_changed")?}));
        }
        let Some(fingerprint) = &target.fingerprint else {
            return Ok(json!({"target":target,"project":entry["project"]}));
        };
        let saved = MobileTaskDeliveryStore::new(path.to_owned())
            .get(&workspace, &key(target))
            .map_err(|_| "unavailable")?
            .filter(|d| &d.fingerprint == fingerprint)
            .ok_or("delivery_changed")?;
        let bytes = serde_json::to_string(&saved.content).map_err(|_| "unavailable")?;
        if *offset > bytes.len() || !bytes.is_char_boundary(*offset) {
            return Err("invalid_request");
        }
        // 64KiB UTF8 content plus JSON escaping remains below the encrypted 512KiB envelope.
        let mut end = bytes.len().min(offset.saturating_add(64 * 1024));
        while !bytes.is_char_boundary(end) {
            end -= 1;
        }
        let acceptance = commands
            .delivery_acceptance(&workspace, &key(target))
            .map_err(|_| "unavailable")?;
        return Ok(
            json!({"target":target,"delivery":{"offset":offset,"total":bytes.len(),"chunk":&bytes[*offset..end],"accepted":acceptance.is_some(),"completed":saved.outcome.state==storage::MobileTurnState::Completed}}),
        );
    }
    let operation = match &request.operation {
        Operation::Decide { operation_id, .. } | Operation::Accept { operation_id, .. } => {
            operation_id
        }
        _ => return Err("invalid_request"),
    };
    if !super::super::command::valid_id(operation) {
        return Err("invalid_request");
    }
    let source = storage::NewMobileCommand {
        device_id: device.into(),
        operation_id: operation.clone(),
        workspace_id: workspace,
        route_id: route.into(),
        owner_id: shared.owner_id.clone(),
        request_hash: hash(request)?,
    };
    // Lookup before checking a vanished approval: a successful old decision must retain its receipt.
    if let Some(existing) = commands.lookup(&source).map_err(|_| "unavailable")? {
        return reservation(shared, existing);
    }
    if let Operation::Decide { decision, .. } = &request.operation {
        if !approval.is_some_and(|a| a.decisions.contains(decision)) {
            return Err("approval_changed");
        }
    }
    let lease = shared.command_lease(device, operation)?;
    match commands.reserve(&source).map_err(|_| "unavailable")? {
        MobileCommandReservation::New => {}
        old => {
            lease.finished.store(true, Ordering::Release);
            return reservation(shared, old);
        }
    }
    let outcome = match &request.operation {
        Operation::Decide { decision, .. } => {
            let call = DecisionCall {
                target: target.clone(),
                decision: *decision,
                source: source.clone(),
                device: device.into(),
                token: token.into(),
                grant_revision: grant.revision,
                lease: lease.clone(),
            };
            super::super::workspace_chat::request(
                shared,
                authorized.to_vec(),
                LiveWorkspaceAction::ActivityDecision(call),
            )
            .map_err(|code| {
                if code == "unavailable" {
                    "outcome_unknown".into()
                } else {
                    code.into()
                }
            })
        }
        Operation::Accept { .. } => {
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
                || !lease.begin()
            {
                Err("unauthorized".into())
            } else {
                commands
                    .accept_delivery(
                        &source,
                        &key(target),
                        target.fingerprint.as_deref().ok_or("invalid_request")?,
                    )
                    .map(|a| a.result())
                    .map_err(|e| match e {
                        storage::StorageError::InvalidMobileCommand(code) => code,
                        _ => "unavailable".into(),
                    })
            }
        }
        _ => Err("invalid_request".into()),
    };
    commands
        .finish(&source, outcome)
        .map_err(|_| "unavailable")?;
    lease.finished.store(true, Ordering::Release);
    let receipt = commands
        .get(device, operation, &source.workspace_id, &shared.owner_id)
        .map_err(|_| "unavailable")?
        .ok_or("unavailable")?;
    Ok(json!({"receipt":super::super::command::receipt_response(shared,receipt)["receipt"]}))
}
fn reservation(shared: &Shared, existing: MobileCommandReservation) -> Result<Value, &'static str> {
    match existing {
        MobileCommandReservation::Existing(receipt) => Ok(
            json!({"receipt":super::super::command::receipt_response(shared,*receipt)["receipt"]}),
        ),
        MobileCommandReservation::Conflict => Err("idempotency_conflict"),
        MobileCommandReservation::New => Err("unavailable"),
    }
}
