use super::*;

impl WorkspaceOperationStore {
    /// Retryable writer transaction: either this whole event batch commits or the caller retains
    /// it. Exact duplicate events are harmless; old evidence cannot regress a known terminal.
    pub fn record_events(&self, events: &[CardInstanceEvent]) -> StorageResult<()> {
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for event in events {
            apply_event(&tx, event)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// No returned item is a delivery instruction. Startup only annotates history; never enqueue.
    pub fn reconcile_interrupted(&self) -> StorageResult<()> {
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids = tx
            .prepare(
                "SELECT assignment_id FROM workspace_operation_assignments ORDER BY assignment_id",
            )?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in ids {
            let Some(item) = query::read_assignment(&tx, &id)? else {
                continue;
            };
            if item.receipt.stage.is_terminal()
                || item.receipt.stage == CardInstanceStage::OutcomeUnknown
            {
                continue;
            }
            let zero = item.input_phase == WorkspaceInputPhase::Queued;
            let mut receipt = item.receipt;
            receipt.stage = if zero {
                CardInstanceStage::Interrupted
            } else {
                CardInstanceStage::OutcomeUnknown
            };
            receipt.waiting_reason = None;
            receipt.failure = Some(CardInstanceFailure {
                code: "app_restarted_no_replay".into(),
                retry_safe: zero,
            });
            let now = now_ms()?;
            tx.execute("UPDATE workspace_operation_assignments SET receipt_json=?2,observed_at_ms=?3 WHERE assignment_id=?1",params![id,serde_json::to_string(&receipt)?,now])?;
            insert_event(
                &tx,
                &CardInstanceEvent {
                    receipt,
                    observed_at_ms: now,
                    evidence: "app_restarted_no_replay".into(),
                },
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Preparation/authorization failure after atomic acceptance remains an accepted item with
    /// an explicit failure. The caller proves zero input; this never deletes the original record.
    pub fn fail_before_input(&self, id: &str, code: &str) -> StorageResult<()> {
        let mut connection = self.write_connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let item = query::read_assignment(&tx, id)?.ok_or_else(|| invalid("assignment_missing"))?;
        if item.receipt.stage.is_terminal() {
            return Ok(());
        }
        if item.input_phase != WorkspaceInputPhase::Queued {
            return Err(invalid("input_may_have_started"));
        }
        let mut receipt = item.receipt;
        receipt.stage = CardInstanceStage::Failed;
        receipt.waiting_reason = None;
        receipt.failure = Some(CardInstanceFailure {
            code: code.into(),
            retry_safe: true,
        });
        apply_event(
            &tx,
            &CardInstanceEvent {
                receipt,
                observed_at_ms: now_ms()?,
                evidence: code.into(),
            },
        )?;
        tx.commit()?;
        Ok(())
    }
}

fn apply_event(connection: &Connection, event: &CardInstanceEvent) -> StorageResult<()> {
    let id = &event.receipt.assignment_id;
    let item =
        query::read_assignment(connection, id)?.ok_or_else(|| invalid("assignment_missing"))?;
    let receipt = &event.receipt;
    let previous = &item.receipt;
    if receipt.target != previous.target
        || receipt.name_at_acceptance != previous.name_at_acceptance
        || receipt.tracking != previous.tracking
    {
        return Err(invalid("event_target_changed"));
    }
    if previous.bound_turn.is_some() && receipt.bound_turn != previous.bound_turn {
        // A delayed pre-binding snapshot has no new turn/result claim. Ignore it rather than
        // poisoning a retryable event batch; another bound turn or result claim stays an error.
        if receipt.bound_turn.is_none()
            && receipt.result.is_none()
            && matches!(
                receipt.stage,
                CardInstanceStage::Accepted
                    | CardInstanceStage::WaitingInput
                    | CardInstanceStage::Submitted
            )
            && stage_rank(receipt.stage) < stage_rank(previous.stage)
            && event.evidence != "provider_result_snapshot"
        {
            return Ok(());
        }
        return Err(invalid("event_turn_changed"));
    }
    if receipt.bound_turn.as_ref().is_some_and(|turn| {
        turn.run_id != receipt.target.execution.run_id
            || receipt.target.execution.session_id.as_deref() != Some(&turn.session_id)
            || turn.turn_id.is_empty()
    }) {
        return Err(invalid("event_turn_changed"));
    }
    if previous.stage.is_terminal() {
        if receipt == previous {
            return insert_event(connection, event);
        }
        if event.evidence == "provider_result_snapshot"
            && receipt.stage == previous.stage
            && previous.bound_turn.is_some()
            && previous.result.is_none()
            && receipt.result.is_some()
        {
            let mut enriched = previous.clone();
            enriched.result = receipt.result.clone();
            if enriched != *receipt {
                return Err(invalid("result_snapshot_changed_receipt"));
            }
            connection.execute("UPDATE workspace_operation_assignments SET receipt_json=?2,observed_at_ms=MAX(observed_at_ms,?3) WHERE assignment_id=?1",params![id,serde_json::to_string(&enriched)?,event.observed_at_ms])?;
            return insert_event(connection, event);
        }
        return Ok(());
    }
    if receipt.stage == CardInstanceStage::Running && receipt.bound_turn.is_none() {
        return Err(invalid("running_without_turn_evidence"));
    }
    let safe_failure = receipt
        .failure
        .as_ref()
        .is_some_and(|failure| failure.retry_safe);
    if receipt.stage.is_terminal() && receipt.bound_turn.is_some() && previous.bound_turn.is_none()
    {
        return Err(invalid("terminal_without_original_turn_binding"));
    }
    if safe_failure
        && (!matches!(
            previous.stage,
            CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
        ) || matches!(
            item.input_phase,
            WorkspaceInputPhase::BodyWritten | WorkspaceInputPhase::EnterConfirmed
        ))
    {
        return Err(invalid("unsafe_retry_qualification"));
    }
    if receipt.stage == CardInstanceStage::Cancelled
        && receipt.bound_turn.is_none()
        && (!matches!(
            previous.stage,
            CardInstanceStage::Accepted | CardInstanceStage::WaitingInput
        ) || matches!(
            item.input_phase,
            WorkspaceInputPhase::BodyWritten | WorkspaceInputPhase::EnterConfirmed
        ))
    {
        return Err(invalid("cancel_without_terminal_evidence"));
    }
    if receipt.stage.is_terminal()
        && !safe_failure
        && receipt.stage != CardInstanceStage::Cancelled
        && receipt.bound_turn.is_none()
    {
        return Err(invalid("terminal_without_turn_evidence"));
    }
    if item.input_phase == WorkspaceInputPhase::Queued
        && matches!(
            receipt.stage,
            CardInstanceStage::Submitted | CardInstanceStage::Running
        )
    {
        return Err(invalid("input_not_armed"));
    }
    if stage_rank(receipt.stage) < stage_rank(previous.stage)
        || previous.stage == CardInstanceStage::OutcomeUnknown && !receipt.stage.is_terminal()
    {
        return Ok(());
    }
    let phase = match event.evidence.as_str() {
        "draft_pasted_enter_pending" if item.input_phase != WorkspaceInputPhase::Queued => {
            WorkspaceInputPhase::BodyWritten
        }
        "enter_confirmed" if item.input_phase != WorkspaceInputPhase::Queued => {
            WorkspaceInputPhase::EnterConfirmed
        }
        _ => item.input_phase,
    };
    connection.execute("UPDATE workspace_operation_assignments SET receipt_json=?2,input_phase=?3,observed_at_ms=MAX(observed_at_ms,?4) WHERE assignment_id=?1",params![id,serde_json::to_string(receipt)?,phase_name(phase),event.observed_at_ms])?;
    insert_event(connection, event)
}

pub(super) fn insert_event(
    connection: &Connection,
    event: &CardInstanceEvent,
) -> StorageResult<()> {
    let json = serde_json::to_string(&event.receipt)?;
    let key = digest(&serde_json::to_vec(event)?)?;
    connection.execute("INSERT OR IGNORE INTO workspace_operation_events(assignment_id,origin_key,operation_id,workspace_id,dedupe_key,evidence,receipt_json,observed_at_ms) SELECT assignment_id,origin_key,operation_id,workspace_id,?2,?3,?4,?5 FROM workspace_operation_assignments WHERE assignment_id=?1",params![event.receipt.assignment_id,key,event.evidence,json,event.observed_at_ms])?;
    Ok(())
}
fn stage_rank(stage: CardInstanceStage) -> u8 {
    match stage {
        CardInstanceStage::Accepted | CardInstanceStage::WaitingInput => 0,
        CardInstanceStage::Submitted => 1,
        CardInstanceStage::Running => 2,
        CardInstanceStage::OutcomeUnknown => 3,
        _ => 4,
    }
}
