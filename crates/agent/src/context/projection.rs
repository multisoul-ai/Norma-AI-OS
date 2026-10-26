use super::*;

/// Pure working-state projection; no operation is executed by recording a fact.
pub fn project_event(
    state: &WorkingState,
    record: &RecordedContextEvent,
) -> Result<WorkingState, &'static str> {
    if !record.event.scope.is_valid()
        || record.reference.scope != record.event.scope
        || record.reference.event_id != record.event.event_id
        || record.event.event_id.trim().is_empty()
        || record.event.event_id.len() > 256
        || record.reference.revision == 0
    {
        return Err("invalid_event_identity");
    }
    if matches!(
        record.event.payload,
        ContextPayload::Goal { .. }
            | ContextPayload::Constraint { .. }
            | ContextPayload::RevokeConstraint { .. }
    ) && matches!(
        record.event.source.origin,
        ContextOrigin::Brain | ContextOrigin::Host
    ) {
        return Err("working_state_requires_user_source");
    }
    let mut next = state.clone();
    match &record.event.payload {
        ContextPayload::UserInput { text } => {
            if matches!(
                record.event.source.origin,
                ContextOrigin::Brain | ContextOrigin::Host
            ) {
                return Err("input_requires_user_source");
            }
            // Match the existing native delegation limit; summaries remain bounded separately.
            if text.trim().is_empty() || text.len() > 32 * 1024 {
                return Err("invalid_user_input");
            }
        }
        ContextPayload::Receipt {
            operation_id,
            summary,
            ..
        } => {
            if record.event.source.origin != ContextOrigin::Host {
                return Err("receipt_requires_host_evidence");
            }
            if operation_id.trim().is_empty() || operation_id.len() > 256 {
                return Err("invalid_operation_id");
            }
            non_blank(summary)?;
        }
        ContextPayload::RevokeConstraint { key, supersedes } => {
            if state.constraints.get(key).map(|value| &value.reference) != Some(supersedes) {
                return Err("stale_constraint_reference");
            }
            next.constraints.remove(key);
        }
        ContextPayload::Goal { text, supersedes } => {
            non_blank(text)?;
            if state.goal.as_ref().map(|value| &value.reference) != supersedes.as_ref() {
                return Err("stale_goal_reference");
            }
            next.goal = Some(ContextFact {
                reference: record.reference.clone(),
                text: text.clone(),
            });
        }
        ContextPayload::Constraint {
            key,
            text,
            enforcement,
            supersedes,
        } => {
            if key.trim().is_empty()
                || key.len() > 128
                || text.trim().is_empty()
                || text.len() > 8192
            {
                return Err("invalid_constraint");
            }
            if let ConstraintEnforcement::Enforced {
                target: Some(target),
                ..
            } = enforcement
            {
                if target.trim().is_empty() || target.len() > 256 {
                    return Err("invalid_constraint_target");
                }
            }
            if state.constraints.get(key).map(|value| &value.reference) != supersedes.as_ref() {
                return Err("stale_constraint_reference");
            }
            if state.constraints.get(key).is_some_and(|previous| {
                matches!(previous.enforcement, ConstraintEnforcement::Enforced { .. })
                    && previous.enforcement != *enforcement
            }) {
                return Err("enforcement_change_requires_revocation");
            }
            next.constraints.insert(
                key.clone(),
                ContextConstraint {
                    reference: record.reference.clone(),
                    text: text.clone(),
                    enforcement: enforcement.clone(),
                },
            );
        }
        ContextPayload::Decision { reason, .. } => {
            non_blank(reason)?;
        }
        ContextPayload::Report { text } => {
            non_blank(text)?;
            next.reports.insert(
                record.event.event_id.clone(),
                ContextReport {
                    reference: record.reference.clone(),
                    text: text.clone(),
                    delivery: ReportDelivery::Created,
                },
            );
            // Projection is only a recent index. Immutable report facts remain in the ledger.
            if next.reports.len() > 32 {
                let id = next
                    .reports
                    .iter()
                    .filter(|(_, report)| report_delivery_is_terminal(report.delivery))
                    .min_by_key(|(_, report)| report.reference.revision)
                    .map(|(id, _)| id.clone())
                    .ok_or("active_report_capacity")?;
                next.reports.remove(&id);
            }
        }
        ContextPayload::Delivery { report, status } => {
            if record.event.source.origin != ContextOrigin::Host {
                return Err("delivery_requires_host_evidence");
            }
            let previous = next
                .reports
                .get_mut(&report.event_id)
                .filter(|value| value.reference == *report)
                .ok_or("unavailable_report")?;
            // Transport acknowledgement is independent evidence, and may arrive after audio.
            // Persist the event without rewinding a playback/terminal projection.
            if *status == ReportDelivery::Acknowledged
                && (previous.delivery == ReportDelivery::Playing
                    || report_delivery_is_terminal(previous.delivery))
            {
                return Ok(next);
            }
            if !delivery_transition(previous.delivery, *status) {
                return Err("invalid_delivery_transition");
            }
            previous.delivery = *status;
        }
    }
    Ok(next)
}

/// The caller must freshly read state and resolve the concrete target before execution.
/// Missing targets fail closed for target-specific restrictions. This is deny-only.
pub fn check_context_operation(
    state: &WorkingState,
    operation: ContextOperation,
    target: Option<&str>,
) -> Result<(), Vec<ContextRef>> {
    let blocked: Vec<_> = state
        .constraints
        .values()
        .filter(|constraint| match &constraint.enforcement {
            ConstraintEnforcement::Communicated => false,
            ConstraintEnforcement::Enforced {
                operation: forbidden,
                target: restricted,
            } => {
                *forbidden == operation
                    && (target.is_none()
                        || restricted
                            .as_deref()
                            .is_none_or(|value| Some(value) == target))
            }
        })
        .map(|constraint| constraint.reference.clone())
        .collect();
    if blocked.is_empty() {
        Ok(())
    } else {
        Err(blocked)
    }
}

fn non_blank(text: &str) -> Result<(), &'static str> {
    if text.trim().is_empty() || text.len() > 8192 {
        Err("invalid_context_text")
    } else {
        Ok(())
    }
}

fn delivery_transition(from: ReportDelivery, to: ReportDelivery) -> bool {
    use ReportDelivery::*;
    if from == to {
        return true;
    }
    match from {
        Completed | Skipped | Interrupted | Failed | Unknown => false,
        _ if to == Interrupted => from == Playing,
        _ if matches!(to, Skipped | Failed | Unknown) => true,
        Created => matches!(to, Queued | Sent | Playing),
        Queued => matches!(to, Sent | Playing),
        Sent => matches!(to, Acknowledged | Playing),
        Acknowledged => to == Playing,
        Playing => to == Completed,
    }
}

/// Unknown is a final, unconfirmed outcome, not an active playback reservation.
/// A new playback attempt requires a new report identity; late callbacks cannot revive it.
pub fn report_delivery_is_terminal(status: ReportDelivery) -> bool {
    matches!(
        status,
        ReportDelivery::Completed
            | ReportDelivery::Skipped
            | ReportDelivery::Interrupted
            | ReportDelivery::Failed
            | ReportDelivery::Unknown
    )
}
