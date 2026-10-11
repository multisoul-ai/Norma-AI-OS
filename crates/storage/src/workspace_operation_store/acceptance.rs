use super::*;
use agent::card_instance::{CardInstanceTracking, CardInstanceTurn};
use std::collections::HashSet;

impl WorkspaceOperationStore {
    /// The single AppDB transaction freezes every assignment or creates none. Only the winner
    /// may install delivery queues; repeated source operations never regain that right.
    pub fn accept(
        &self,
        request: &NewWorkspaceOperation,
    ) -> StorageResult<WorkspaceOperationAcceptance> {
        request.source.validate().map_err(invalid)?;
        validate_original_fingerprint(&request.original_request_fingerprint)?;
        let fingerprint = workspace_operation_fingerprint(&request.source, &request.assignments)?;
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(receipt) = query::read_operation(&tx, &request.source)? {
            if receipt.request_fingerprint != fingerprint
                || receipt.original_request_fingerprint != request.original_request_fingerprint
            {
                return Err(invalid("idempotency_conflict"));
            }
            return Ok(WorkspaceOperationAcceptance {
                newly_accepted: false,
                receipt,
            });
        }
        if tx.query_row("SELECT EXISTS(SELECT 1 FROM workspace_operations WHERE origin_key=?1 AND operation_id=?2)",params![request.source.origin.key(),request.source.operation_id],|r|r.get::<_,bool>(0))? { return Err(invalid("idempotency_scope_conflict")); }
        validate_group(&tx, request)?;
        let now = now_ms()?;
        let origin = request.source.origin.key();
        tx.execute("INSERT INTO workspace_operations(origin_key,operation_id,workspace_id,source_json,authorization_json,original_request_fingerprint,request_fingerprint,accepted_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![origin,request.source.operation_id,request.source.workspace_id,serde_json::to_string(&request.source)?,serde_json::to_string(&request.authorization)?,request.original_request_fingerprint,fingerprint,now])?;
        for (ordinal, item) in request.assignments.iter().enumerate() {
            let assignment = &item.assignment;
            let receipt = CardInstanceReceipt {
                assignment_id: assignment.assignment_id.clone(),
                target: assignment.target.clone(),
                name_at_acceptance: assignment.name_at_acceptance.clone(),
                stage: CardInstanceStage::Accepted,
                tracking: assignment.tracking,
                bound_turn: None,
                cancel_requested: false,
                result: None,
                failure: None,
                waiting_reason: None,
            };
            tx.execute("INSERT INTO workspace_operation_assignments(assignment_id,origin_key,operation_id,workspace_id,ordinal,prepared_json,receipt_json,input_phase,accepted_at_ms,observed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,'queued',?8,?8)",params![assignment.assignment_id,origin,request.source.operation_id,request.source.workspace_id,ordinal as u64,serde_json::to_string(item)?,serde_json::to_string(&receipt)?,now])?;
            events::insert_event(
                &tx,
                &CardInstanceEvent {
                    receipt,
                    observed_at_ms: now,
                    evidence: "accepted_atomic".into(),
                },
            )?;
        }
        let receipt = query::read_operation(&tx, &request.source)?
            .ok_or_else(|| invalid("receipt_missing"))?;
        tx.commit()?;
        Ok(WorkspaceOperationAcceptance {
            newly_accepted: true,
            receipt,
        })
    }

    /// Commit the possibility of input before any paste/RPC/Enter. Returns false when already
    /// armed; duplicate/restarted callers are forbidden from replaying the same transmission.
    pub fn arm_input(&self, assignment_id: &str) -> StorageResult<bool> {
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let item = query::read_assignment(&tx, assignment_id)?
            .ok_or_else(|| invalid("assignment_missing"))?;
        if item.input_phase != WorkspaceInputPhase::Queued {
            return Ok(false);
        }
        if !matches!(
            item.receipt.stage,
            CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
        ) {
            return Err(invalid("assignment_not_deliverable"));
        }
        validate_persisted_authority(&tx, &item)?;
        tx.execute("UPDATE workspace_operation_assignments SET input_phase='armed' WHERE assignment_id=?1 AND input_phase='queued'", [assignment_id])?;
        events::insert_event(
            &tx,
            &CardInstanceEvent {
                receipt: item.receipt,
                observed_at_ms: now_ms()?,
                evidence: "input_armed_before_side_effect".into(),
            },
        )?;
        tx.commit()?;
        Ok(true)
    }
}

fn validate_group(connection: &Connection, request: &NewWorkspaceOperation) -> StorageResult<()> {
    if request.assignments.is_empty() || request.assignments.len() > 64 {
        return Err(invalid("invalid_batch_size"));
    }
    let revision: Option<u64> = connection
        .query_row(
            "SELECT revision FROM workspace_agent_role_revisions WHERE workspace_id=?1",
            [&request.source.workspace_id],
            |r| r.get(0),
        )
        .optional()?;
    if revision != Some(request.expected_role_revision) {
        return Err(invalid("directory_changed"));
    }
    validate_authorization(
        connection,
        &request.source,
        &request.authorization,
        &request.assignments,
    )?;
    let mut ids = HashSet::new();
    let mut tasks = HashSet::new();
    let mut logical_tasks = HashSet::new();
    let mut untracked = HashSet::new();
    for item in &request.assignments {
        let assignment = &item.assignment;
        assignment.validate().map_err(|e| invalid(e.code()))?;
        if !ids.insert(&assignment.assignment_id)
            || !tasks.insert(&item.task_ref)
            || !logical_tasks.insert(&item.logical_task_ref)
            || assignment.target.card.workspace_id != request.source.workspace_id
            || item.task_ref.is_empty()
            || item.task_ref.len() > 512
            || item.project_id.is_empty()
        {
            return Err(invalid("invalid_assignment_group"));
        }
        if assignment.force_untracked
            && !explicit_force(
                &request.source.original_text,
                &assignment.name_at_acceptance,
            )
        {
            return Err(invalid("force_not_authorized_by_user"));
        }
        if assignment.tracking == CardInstanceTracking::Untracked
            && !assignment.force_untracked
            && !untracked.insert(&assignment.target)
        {
            return Err(invalid("untracked_group_conflict"));
        }
        validate_target(connection, item)?;
        validate_lineage(connection, item)?;
        if let WorkspaceAssignmentKind::Reply {
            parent_assignment_id,
            turn,
        } = &item.kind
        {
            validate_reply(connection, item, parent_assignment_id, turn)?;
        }
    }
    Ok(())
}

fn validate_target(
    connection: &Connection,
    item: &WorkspaceOperationAssignment,
) -> StorageResult<()> {
    let card = &item.assignment.target.card;
    let active: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM workspace_agent_roles r JOIN desktop_project_areas a ON a.desktop_workspace_id=r.workspace_id AND a.id=r.area_id WHERE r.workspace_id=?1 AND r.area_id=?2 AND r.node_id=?3 AND r.card_instance_id=?4 AND r.enabled=1 AND a.project_key=?5)",params![card.workspace_id,card.area_id,card.node_id,card.card_instance_id,item.project_id],|r|r.get(0))?;
    active
        .then_some(())
        .ok_or_else(|| invalid("target_changed_or_disabled"))
}

fn validate_reply(
    connection: &Connection,
    item: &WorkspaceOperationAssignment,
    parent: &str,
    turn: &CardInstanceTurn,
) -> StorageResult<()> {
    let original =
        query::read_assignment(connection, parent)?.ok_or_else(|| invalid("reply_task_missing"))?;
    if original.logical_task_ref != item.logical_task_ref
        || original.assignment.target != item.assignment.target
        || original.receipt.bound_turn.as_ref() != Some(turn)
    {
        return Err(invalid("reply_turn_or_target_changed"));
    }
    Ok(())
}

fn explicit_force(original: &str, name: &str) -> bool {
    original.lines().any(|line| {
        let line = line.trim();
        [format!("仍然发送给 {name}"), format!("仍然发送给{name}")]
            .iter()
            .any(|phrase| {
                line.strip_prefix(phrase).is_some_and(|tail| {
                    tail.is_empty()
                        || tail.starts_with(|c: char| {
                            c.is_whitespace() || matches!(c, '，' | ',' | '：' | ':')
                        })
                })
            })
    })
}

fn validate_persisted_authority(
    connection: &Connection,
    item: &StoredWorkspaceAssignment,
) -> StorageResult<()> {
    let (source, auth): (String,String) = connection.query_row("SELECT o.source_json,o.authorization_json FROM workspace_operations o JOIN workspace_operation_assignments a ON a.origin_key=o.origin_key AND a.operation_id=o.operation_id WHERE a.assignment_id=?1",[&item.assignment.assignment_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    validate_target(connection, item)?;
    validate_authorization(
        connection,
        &serde_json::from_str(&source)?,
        &serde_json::from_str(&auth)?,
        std::slice::from_ref(&item.prepared),
    )
}

fn validate_authorization(
    connection: &Connection,
    source: &WorkspaceOperationSource,
    authorization: &WorkspaceOperationAuthorization,
    items: &[WorkspaceOperationAssignment],
) -> StorageResult<()> {
    match (&source.origin, authorization) {
        (WorkspaceOperationOrigin::Desktop { .. }, WorkspaceOperationAuthorization::Desktop) => {
            Ok(())
        }
        (
            WorkspaceOperationOrigin::Mobile { device_id },
            WorkspaceOperationAuthorization::Mobile {
                device_id: authorized_device,
                revision,
                authorized_area_ids,
            },
        ) if device_id == authorized_device => {
            let grant: Option<(u64,bool,String,Option<String>)> = connection.query_row("SELECT revision,revoked,permissions_json,all_projects_access FROM mobile_devices WHERE device_id=?1",[device_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
            let Some((current, revoked, json, all)) = grant else {
                return Err(invalid("unauthorized"));
            };
            if current != *revision || revoked {
                return Err(invalid("authorization_changed"));
            }
            let projects: Vec<crate::MobileProjectAccess> = serde_json::from_str(&json)?;
            for item in items {
                if !authorized_area_ids.contains(&item.assignment.target.card.area_id) {
                    return Err(invalid("unauthorized_area"));
                }
                if all.as_deref() == Some("control") {
                    continue;
                }
                let project_id: Option<String> = connection.query_row("SELECT i.workspace_id FROM workspace_identities i JOIN desktop_project_areas a ON i.canonical_root=a.root_path WHERE a.desktop_workspace_id=?1 AND a.id=?2",params![source.workspace_id,item.assignment.target.card.area_id],|r|r.get(0)).optional()?;
                if !project_id.is_some_and(|id| {
                    projects.iter().any(|p| {
                        p.workspace_id == id && p.access == crate::MobilePermission::Control
                    })
                }) {
                    return Err(invalid("unauthorized"));
                }
            }
            Ok(())
        }
        _ => Err(invalid("origin_authorization_mismatch")),
    }
}

fn validate_lineage(
    connection: &Connection,
    item: &WorkspaceOperationAssignment,
) -> StorageResult<()> {
    let task_exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM workspace_operation_assignments WHERE json_extract(prepared_json,'$.task_ref')=?1)",[&item.task_ref],|r|r.get(0))?;
    if task_exists {
        return Err(invalid("task_reference_conflict"));
    }
    if item.logical_task_ref.is_empty()
        || item.logical_task_ref.len() > 512
        || item.attempt_sequence == 0
    {
        return Err(invalid("invalid_task_lineage"));
    }
    let parent = match &item.kind {
        WorkspaceAssignmentKind::Task => {
            let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM workspace_operation_assignments WHERE json_extract(prepared_json,'$.logical_task_ref')=?1)",[&item.logical_task_ref],|r|r.get(0))?;
            if exists || item.attempt_sequence!=1 || item.task_ref!=item.logical_task_ref { return Err(invalid("invalid_new_task_lineage")); }
            return Ok(());
        }
        WorkspaceAssignmentKind::Reply { parent_assignment_id, .. } => query::read_assignment(connection,parent_assignment_id)?,
        WorkspaceAssignmentKind::Retry { previous_assignment_id } => query::read_assignment(connection,previous_assignment_id)?,
    }.ok_or_else(|| invalid("parent_assignment_missing"))?;
    if parent.assignment.target.card != item.assignment.target.card
        || parent.project_id != item.project_id
        || parent.logical_task_ref != item.logical_task_ref
    {
        return Err(invalid("parent_target_changed"));
    }
    if matches!(item.kind, WorkspaceAssignmentKind::Retry { .. })
        && !(matches!(
            parent.receipt.stage,
            CardInstanceStage::Failed | CardInstanceStage::Interrupted
        ) && parent
            .receipt
            .failure
            .as_ref()
            .is_some_and(|f| f.retry_safe))
    {
        return Err(invalid("retry_not_safe"));
    }
    let latest: u64 = connection.query_row("SELECT COALESCE(MAX(json_extract(prepared_json,'$.attempt_sequence')),0) FROM workspace_operation_assignments WHERE json_extract(prepared_json,'$.logical_task_ref')=?1",[&item.logical_task_ref],|r|r.get(0))?;
    if latest.checked_add(1) != Some(item.attempt_sequence) || item.task_ref == parent.task_ref {
        return Err(invalid("attempt_sequence_conflict"));
    }
    Ok(())
}
