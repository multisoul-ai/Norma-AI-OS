use super::*;

pub(super) fn state_views(
    request: &AssemblyRequest,
) -> Result<Vec<ContextStateView>, ContextAssemblyError> {
    let mut snapshots: Vec<&ContextSnapshot> = Vec::new();
    for snapshot in &request.snapshots {
        if !snapshot.scope.readable_from(&request.scope) {
            continue;
        }
        if let Some(previous) = snapshots
            .iter()
            .find(|previous| previous.scope == snapshot.scope)
        {
            if **previous != *snapshot {
                return Err(ContextAssemblyError::Invalid("conflicting_snapshot"));
            }
            continue;
        }
        let valid_ref = |reference: &ContextRef| {
            reference.scope == snapshot.scope
                && reference.revision > 0
                && reference.revision <= snapshot.revision
                && !reference.event_id.trim().is_empty()
        };
        if snapshot
            .state
            .goal
            .as_ref()
            .is_some_and(|goal| !valid_ref(&goal.reference))
            || snapshot
                .state
                .constraints
                .values()
                .any(|value| !valid_ref(&value.reference))
            || snapshot
                .state
                .reports
                .iter()
                .any(|(id, value)| !valid_ref(&value.reference) || id != &value.reference.event_id)
        {
            return Err(ContextAssemblyError::Invalid("snapshot_source_mismatch"));
        }
        snapshots.push(snapshot);
    }
    snapshots.sort_by_key(|snapshot| serde_json::to_string(&snapshot.scope).unwrap_or_default());
    Ok(snapshots
        .into_iter()
        .map(|snapshot| {
            let mut reports: Vec<_> = snapshot.state.reports.values().collect();
            reports.sort_by_key(|report| std::cmp::Reverse(report.reference.revision));
            // Only four pointers in the fixed group; bodies and older reports stay on demand.
            let recent_reports = reports
                .into_iter()
                .take(4)
                .map(|report| ContextReportIndex {
                    reference: report.reference.clone(),
                    delivery: report.delivery,
                })
                .collect();
            let mut state = snapshot.state.clone();
            state.reports.clear();
            ContextStateView {
                scope: snapshot.scope.clone(),
                revision: snapshot.revision,
                state,
                recent_reports,
            }
        })
        .collect())
}
