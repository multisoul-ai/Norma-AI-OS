use super::{Notice, Shared};
use crate::mobile::{LiveApprovalRequest, LiveSnapshotRequest};
use norma::mobile::{ApprovalListRequest, ApprovalSnapshot, ProjectSnapshot};
use std::time::{Duration, Instant};

pub(super) fn snapshots(shared: &Shared, workspaces: &[String]) -> Option<Vec<ProjectSnapshot>> {
    let identities = shared.access().ok()?.store.workspace_identity_store();
    let projects = workspaces
        .iter()
        .map(|id| {
            Some(ProjectSnapshot {
                workspace_id: id.clone(),
                display_name: identities.get(id).ok()??.display_name,
                state: "unavailable".into(),
                route_id: None,
                card_count: None,
                cards: None,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let (reply, result) = crossbeam_channel::bounded(1);
    // Existing snapshot budget: 2s for the foreground owner, no database lock held while waiting.
    let deadline = Instant::now() + Duration::from_secs(2);
    shared
        .snapshots
        .as_ref()?
        .try_send(LiveSnapshotRequest {
            projects,
            include_cards: true,
            agents: None,
            deadline,
            reply,
        })
        .ok()?;
    result
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
}

pub(super) fn approvals(
    shared: &Shared,
    workspace: &str,
    route: &str,
) -> Option<Vec<ApprovalSnapshot>> {
    let (reply, result) = crossbeam_channel::bounded(1);
    // Existing approval read budget; stale replies do not produce a notification.
    let deadline = Instant::now() + Duration::from_secs(4);
    shared
        .approvals
        .as_ref()?
        .try_send(LiveApprovalRequest {
            request: ApprovalListRequest {
                workspace_id: workspace.into(),
                route_id: route.into(),
            },
            deadline,
            reply,
        })
        .ok()?;
    result
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()?
        .ok()
}

pub(super) fn still_current(shared: &Shared, notice: &Notice) -> bool {
    let Some(projects) = snapshots(shared, std::slice::from_ref(&notice.workspace)) else {
        return false;
    };
    let Some(project) = projects
        .iter()
        .find(|project| project.workspace_id == notice.workspace)
    else {
        return false;
    };
    if !matches_card(project, notice) {
        return false;
    }
    if let Some(request) = &notice.request {
        return approvals(shared, &notice.workspace, &notice.route).is_some_and(|items| {
            items.iter().any(|item| {
                item.target.valid()
                    && !item.decisions.is_empty()
                    && item.target.node_id == notice.key.node_id
                    && item.target.profile == notice.key.profile
                    && item.target.run_id == notice.key.run_id
                    && item.target.session_id == notice.key.session_id
                    && item.target.turn_id == notice.key.turn_id
                    && item.target.request_id == *request
            })
        });
    }
    true
}

pub(super) fn matches_card(project: &ProjectSnapshot, notice: &Notice) -> bool {
    project.state == "open"
        && project.route_id.as_deref() == Some(&notice.route)
        && project.cards.as_ref().is_some_and(|cards| {
            cards.iter().any(|card| {
                card.mobile_control
                    && card.node_id == notice.key.node_id
                    && card.profile == notice.key.profile
                    && card.run_id.as_deref() == Some(&notice.key.run_id)
                    && card.session_id.as_deref() == Some(&notice.key.session_id)
            })
        })
}
