use super::{conversation::send, failure, success, Action, Request, Shared};
use crate::mobile::{LiveReviewRequest, ProjectReviewRequest};
use norma::mobile::{ConversationFragment, MobileSocket};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) fn serve(
    socket: &mut MobileSocket,
    shared: &Arc<Shared>,
    socket_id: u64,
    request: Request,
) {
    if let Err(code) = run(socket, shared, socket_id, &request) {
        let _ = send(socket, failure(&request.request_id, code));
    }
}
fn run(
    socket: &mut MobileSocket,
    shared: &Shared,
    socket_id: u64,
    request: &Request,
) -> Result<(), &'static str> {
    let Action::ProjectReview {
        device_id,
        token,
        request: scope,
    } = &request.action
    else {
        return Err("invalid_request");
    };
    if !scope.valid() {
        return Err("invalid_request");
    }
    let revision = authorize(shared, request, scope, device_id, token)?;
    let result = if let Some(key) = &scope.delivery_key {
        let deliveries = recover_deliveries(shared, request, scope, device_id, token, revision)?;
        let database = shared
            .access()
            .map_err(|_| "unavailable")?
            .store
            .path()
            .to_path_buf();
        let saved = deliveries
            .get(&scope.workspace_id, key)
            .map_err(|_| "unavailable")?
            .ok_or("not_found")?;
        let storage::TaskDeliveryContent::Available { review } = saved.content else {
            return Err("delivery_unavailable");
        };
        let acceptance = storage::MobileCommandStore::new(database)
            .delivery_acceptance(&scope.workspace_id, key)
            .map_err(|_| "unavailable")?;
        json!({"request":scope,"review":review,"delivery":{"outcome":saved.outcome,"fingerprint":saved.fingerprint,"saved_at_unix":saved.saved_at_unix,"acceptance":acceptance}})
    } else {
        verify_route(shared, request, scope, device_id, token, revision)?;
        json!({"request":scope,"review":capture(shared, scope)?})
    };
    if authorize(shared, request, scope, device_id, token)? != revision {
        return Err("unauthorized");
    }
    verify_route(shared, request, scope, device_id, token, revision)?;
    let payload = result.to_string();
    if payload.len() <= 480 * 1024 {
        return send(
            socket,
            success(
                &request.request_id,
                json!({"status":"project_review","review":result}),
            ),
        );
    }
    // Reuse the existing UTF8 fragment codec and32MiB ceiling; no cache, daemon or partial results.
    if payload.len() > norma::mobile::MAX_CONVERSATION_BYTES {
        return Err("review_too_large");
    }
    shared
        .sockets
        .lock()
        .map_err(|_| "unavailable")?
        .get_mut(&socket_id)
        .ok_or("unavailable")?
        .device_id = Some(device_id.into());
    let transfer_id =
        crate::norma::endpoint::new_workspace_route_id().map_err(|_| "unavailable")?;
    let mut offset = 0;
    while offset < payload.len() {
        if authorize(shared, request, scope, device_id, token)? != revision {
            return Err("unauthorized");
        }
        let part = ConversationFragment::at(&transfer_id, &payload, offset)?;
        offset += part.text.len();
        if offset == payload.len() {
            verify_route(shared, request, scope, device_id, token, revision)?;
        }
        send(
            socket,
            success(
                &request.request_id,
                json!({"status":"review_fragment","fragment":part}),
            ),
        )?;
    }
    Ok(())
}
pub(super) fn authorize(
    shared: &Shared,
    request: &Request,
    scope: &ProjectReviewRequest,
    device: &str,
    token: &str,
) -> Result<u64, &'static str> {
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(&request.machine_id, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if !grant.allows(&scope.workspace_id, false) {
        return Err("unauthorized");
    }
    Ok(grant.revision)
}
pub(super) fn verify_route(
    shared: &Shared,
    request: &Request,
    scope: &ProjectReviewRequest,
    device: &str,
    token: &str,
    revision: u64,
) -> Result<serde_json::Value, &'static str> {
    let current = crate::mobile::snapshot::read(
        shared,
        &request.machine_id,
        device,
        token,
        Some(&scope.workspace_id),
    )?;
    if current["grant"]["revision"].as_u64() != Some(revision) {
        return Err("unauthorized");
    }
    let project = &current["projects"][0];
    if project["state"] != "open" || project["route_id"] != scope.route_id {
        return Err("route_expired");
    }
    Ok(current)
}

fn capture(
    shared: &Shared,
    scope: &ProjectReviewRequest,
) -> Result<workspace::review::ProjectReview, &'static str> {
    // Up to128 files × up to6 short processes can exceed4s even without large content.
    // Allow10s for capture; the native first-frame budget includes both2s route checks.
    let deadline = Instant::now() + Duration::from_secs(10);
    let (reply, receiver) = crossbeam_channel::bounded(1);
    shared
        .reviews
        .as_ref()
        .ok_or("unavailable")?
        .try_send(LiveReviewRequest {
            request: scope.clone(),
            deadline,
            reply,
        })
        .map_err(|_| "unavailable")?;
    receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "review_expired")?
}

pub(super) fn list(shared: &Shared, request: &Request) -> Result<serde_json::Value, &'static str> {
    let Action::TaskDeliveries {
        device_id,
        token,
        request: scope,
        node_id,
        before,
        pending_only,
        ..
    } = &request.action
    else {
        return Err("invalid_request");
    };
    if !scope.valid()
        || scope.delivery_key.is_some()
        || node_id.is_some_and(|n| n == 0 || n > i64::MAX as u64)
        || before.is_some_and(|n| n == 0 || n > i64::MAX as u64)
    {
        return Err("invalid_request");
    }
    let revision = authorize(shared, request, scope, device_id, token)?;
    let deliveries = recover_deliveries(shared, request, scope, device_id, token, revision)?;
    let (items, pending_count) = deliveries
        .page(&scope.workspace_id, *node_id, *before, *pending_only)
        .map_err(|_| "unavailable")?;
    if authorize(shared, request, scope, device_id, token)? != revision {
        return Err("unauthorized");
    }
    let current = verify_route(shared, request, scope, device_id, token, revision)?;
    let collecting = current["projects"][0]["cards"]
        .as_array()
        .is_some_and(|cards| {
            cards.iter().any(|card| {
                card["state"] == "collecting"
                    && node_id.is_none_or(|node| card["node_id"].as_u64() == Some(node))
            })
        });
    let cursor = (items.len() == 20)
        .then(|| items.last().map(|d| d.sequence))
        .flatten();
    Ok(
        json!({"status":"task_deliveries","grant":current["grant"],"deliveries":{"request":scope,"node_id":node_id,"items":items,"next_cursor":cursor,"pending_only":pending_only,"pending_count":pending_count,"collecting":collecting}}),
    )
}

fn recover_deliveries(
    shared: &Shared,
    request: &Request,
    scope: &ProjectReviewRequest,
    device: &str,
    token: &str,
    revision: u64,
) -> Result<storage::MobileTaskDeliveryStore, &'static str> {
    let database = shared
        .access()
        .map_err(|_| "unavailable")?
        .store
        .path()
        .to_path_buf();
    let store = storage::MobileTaskDeliveryStore::new(database);
    let candidates = store
        .recovery_candidates(&scope.workspace_id, &scope.route_id)
        .map_err(|_| "unavailable")?;
    // Verify after taking the candidate snapshot, excluding commands from any later route.
    verify_route(shared, request, scope, device, token, revision)?;
    for key in candidates {
        let result = store.record(
            &scope.workspace_id,
            &key,
            &storage::TaskDeliveryContent::Unavailable {
                reason: "window_closed".into(),
            },
        );
        if result.is_err()
            && store
                .get(&scope.workspace_id, &key)
                .map_err(|_| "unavailable")?
                .is_none()
        {
            return Err("unavailable");
        }
        // An already frozen result wins a concurrent race; record never overwrites it.
    }
    Ok(store)
}
