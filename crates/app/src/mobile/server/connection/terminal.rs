use super::{conversation::send, failure, success, Action, Request, Shared};
use norma::mobile::{MobileSocket, TerminalProjection};
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
    let Action::Terminal {
        device_id,
        token,
        request: scope,
        watch,
    } = &request.action
    else {
        return Err("invalid_request");
    };
    if !scope.valid() || (*watch && scope.before.is_some()) {
        return Err("invalid_request");
    }
    let authorize = || -> Result<u64, &'static str> {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(&request.machine_id, device_id, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if !grant.allows(&scope.workspace_id, false) {
            return Err("unauthorized");
        }
        shared
            .sockets
            .lock()
            .map_err(|_| "unavailable")?
            .get_mut(&socket_id)
            .ok_or("unavailable")?
            .device_id = Some(device_id.clone());
        Ok(grant.revision)
    };
    let grant_revision = authorize()?;
    let mut projection = TerminalProjection::new(
        crate::norma::endpoint::new_workspace_route_id().map_err(|_| "unavailable")?,
        scope.clone(),
    );
    let mut sent_at = Instant::now();
    let waiter = watch
        .then(|| super::watch_wait::WatchWait::new(socket))
        .transpose()?;
    loop {
        if authorize()? != grant_revision {
            return Err("unauthorized");
        }
        let revision = crate::mobile::project_events::revision();
        let page = crate::mobile::terminal::read(shared, scope.clone())?;
        if authorize()? != grant_revision {
            return Err("unauthorized");
        }
        let (changed, result) = if *watch {
            let event = projection.update(page)?;
            (
                event.page.is_some(),
                json!({"status":"terminal_event", "terminal_event":event}),
            )
        } else {
            (
                true,
                json!({"status":"terminal", "terminal":{"request":scope,"page":page}}),
            )
        };
        if changed || sent_at.elapsed() >= Duration::from_secs(2) {
            let response = success(&request.request_id, result);
            if response.to_string().len() > 500 * 1024 {
                return Err("terminal_unavailable");
            }
            send(socket, response)?;
            sent_at = Instant::now();
        }
        let Some(waiter) = &waiter else {
            return Ok(());
        };
        waiter.wait(socket, shared, revision, sent_at)?;
    }
}
