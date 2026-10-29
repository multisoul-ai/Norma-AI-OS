//! Authenticate on the foreground, read exact references off-thread, recheck before replying.
use super::{resolve_targets, routing_error_response};
use gpui::App;
use norma::protocol::{
    BrainOp, CardCommand, CardCommandError, CardCommandResult, WireRequest, WireResponse,
};

pub(super) fn dispatch(
    request: WireRequest,
    response: crossbeam_channel::Sender<WireResponse>,
    cx: &mut App,
) {
    let id = request.request_id;
    let CardCommand::Brain {
        token,
        op: BrainOp::Context { reference },
    } = request.command.clone()
    else {
        return;
    };
    let target = match resolve_targets(&request, cx) {
        Ok(routed) => routed.target,
        Err(error) => {
            let _ = response.send(routing_error_response(id, error));
            return;
        }
    };
    let prepared = target
        .app
        .update(cx, |app, _| app.prepare_brain_context(&token))
        .map_err(|_| "目标项目已关闭".to_string())
        .and_then(|r| r.map_err(|e| e.to_string()));
    let service = match prepared {
        Ok(service) => service,
        Err(message) => {
            let _ = response.send(failure(id, message));
            return;
        }
    };
    cx.spawn(async move |cx| {
        let result = service.run(None, move |access| {
            let record = access.read(&reference)?;
            let snapshot = access.snapshot()?;
            // Preserve scope/revision and effective rules; do not return unrelated report bodies.
            let mut state = snapshot.state;
            state.reports.clear();
            Ok(serde_json::json!({"record":record,"current_state":{"scope":snapshot.scope,"revision":snapshot.revision,"state":state}}).to_string())
        }).await;
        let current = target.app.update(cx, |app, _| app.prepare_brain_context(&token).is_ok_and(|current| current.scope() == service.scope())).unwrap_or(false);
        let reply = if !current { failure(id, "大脑令牌或来源项目已失效".into()) } else {
            match result { Ok(output) => WireResponse::new(id, Ok(CardCommandResult::Brain { output })), Err(error) => failure(id, error.to_string()) }
        };
        let _ = response.send(reply);
    }).detach();
}

fn failure(id: u64, message: String) -> WireResponse {
    WireResponse::new(id, Err(CardCommandError::Unsupported { message }))
}

/// A refusal must reach Runtime immediately, even if context storage is unavailable.
/// Capture scope and execute in one foreground turn, then save only the accepted result.
pub(super) fn dispatch_denied(
    request: WireRequest,
    response: crossbeam_channel::Sender<WireResponse>,
    cx: &mut App,
) {
    let id = request.request_id;
    let CardCommand::Brain { token, op } = &request.command else {
        return;
    };
    let op = op.clone();
    let target = match resolve_targets(&request, cx) {
        Ok(routed) => routed.target,
        Err(error) => {
            let _ = response.send(routing_error_response(id, error));
            return;
        }
    };
    let service = target
        .app
        .update(cx, |app, _| app.prepare_brain_context(token))
        .map_err(|_| "目标项目已关闭".to_string())
        .and_then(|r| r.map_err(|e| e.to_string()));
    let mut reply = super::execute_in_workspace(target, request, cx);
    cx.spawn(async move |_| {
        if let Ok(result) = &mut reply.result {
            match service {
                Ok(service) => {
                    service.save_brain_action_result(op, result).await;
                }
                Err(error) => crate::app::brain_action_context_failure(result, error),
            }
        }
        let _ = response.send(reply);
    })
    .detach();
}

/// Cold projects must finish loading effective restrictions before an autonomous
/// action reaches the synchronous gate. Token/scope are rechecked after the await.
pub(super) fn dispatch_guarded(
    request: WireRequest,
    response: crossbeam_channel::Sender<WireResponse>,
    cx: &mut App,
) {
    let id = request.request_id;
    let CardCommand::Brain { token, op } = &request.command else {
        return;
    };
    let token = token.clone();
    let op = op.clone();
    let target = match resolve_targets(&request, cx) {
        Ok(routed) => routed.target,
        Err(error) => {
            let _ = response.send(routing_error_response(id, error));
            return;
        }
    };
    let service = match target
        .app
        .update(cx, |app, _| app.prepare_brain_context(&token))
    {
        Ok(Ok(service)) => service,
        other => {
            let message = match other {
                Ok(Err(error)) => error.to_string(),
                _ => "目标项目已关闭".into(),
            };
            let _ = response.send(failure(id, message));
            return;
        }
    };
    cx.spawn(async move |cx| {
        if let Err(error) = service.snapshot().await {
            let _ = response.send(failure(id, error.to_string()));
            return;
        }
        let mut reply = cx
            .update(|cx| {
                let current = target
                    .app
                    .update(cx, |app, _| {
                        app.prepare_brain_context(&token)
                            .is_ok_and(|current| current.scope() == service.scope())
                    })
                    .unwrap_or(false);
                if !current {
                    return failure(id, "大脑令牌或来源项目已失效".into());
                }
                super::execute_in_workspace(target, request, cx)
            })
            .unwrap_or_else(|_| failure(id, "目标项目已关闭".into()));
        if let Ok(result) = &mut reply.result {
            service.save_brain_action_result(op, result).await;
        }
        let _ = response.send(reply);
    })
    .detach();
}
