use super::{conversation, failure, Action, Request, Shared};
use norma::mobile::{ConversationPage, ConversationProjection, MobileSocket};
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
        let _ = conversation::send(socket, failure(&request.request_id, code));
    }
}
fn run(
    socket: &mut MobileSocket,
    shared: &Arc<Shared>,
    socket_id: u64,
    request: &Request,
) -> Result<(), &'static str> {
    let Action::WatchConversation {
        device_id,
        token,
        request: scope,
    } = &request.action
    else {
        return Err("invalid_request");
    };
    if !scope.valid() || scope.cursor.is_some() {
        return Err("invalid_request");
    }
    let grant_revision = conversation::authorize(shared, request, socket_id)?;
    let id = crate::norma::endpoint::new_workspace_route_id().map_err(|_| "unavailable")?;
    let mut projection = ConversationProjection::new(id, scope.clone());
    let mut read = scope.clone();
    let mut sent_at = Instant::now();
    let waiter = super::watch_wait::WatchWait::new(socket)?;
    loop {
        let revision = crate::mobile::project_events::revision();
        let mut response = crate::mobile::conversation::read(
            shared,
            &request.machine_id,
            device_id,
            token,
            read.clone(),
            None,
        )?;
        let page: ConversationPage =
            serde_json::from_value(response["conversation"]["page"].take())
                .map_err(|_| "invalid_history")?;
        let continuing = page.direction == "newer" && page.has_more && !page.pending_tail;
        if page.direction == "newer" {
            read.cursor = page.cursor.clone();
        }
        // ponytail: Codex sends the changed recent20-turn page; use Provider item deltas if bandwidth
        // is measured to dominate. Claude already advances its exact full-record cursor.
        let event = projection.update(page)?;
        if event.page.is_some() || sent_at.elapsed() >= Duration::from_secs(2) {
            if conversation::verify_live(shared, request, scope, device_id, token)?
                != grant_revision
            {
                return Err("unauthorized");
            }
            conversation::send_result(
                socket,
                shared,
                socket_id,
                request,
                json!({"status":"conversation_event","conversation_event":event}),
                "conversation_event",
            )?;
            sent_at = Instant::now();
        }
        if !continuing {
            waiter.wait(socket, shared, revision, sent_at)?;
        }
    }
}
