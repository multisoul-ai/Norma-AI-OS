//! `norma brain history` 的端点入口：前台准备读取，后台读完再回复，不阻塞界面。

use super::{resolve_targets, routing_error_response};
use gpui::App;
use norma::protocol::{
    AgentErrorCode, BrainOp, CardCommand, CardCommandError, CardCommandResult, WireRequest,
    WireResponse,
};

fn failure(request_id: u64, message: String) -> WireResponse {
    WireResponse::new(
        request_id,
        Err(CardCommandError::agent(
            AgentErrorCode::InternalError,
            &message,
            false,
        )),
    )
}

pub(super) fn dispatch(
    request: WireRequest,
    response: crossbeam_channel::Sender<WireResponse>,
    cx: &mut App,
) {
    let request_id = request.request_id;
    let CardCommand::Brain {
        token,
        op: BrainOp::History { agent, last, grep },
    } = request.command.clone()
    else {
        return;
    };
    let target = match resolve_targets(&request, cx) {
        Ok(routed) => routed.target,
        Err(error) => {
            let _ = response.send(routing_error_response(request_id, error));
            return;
        }
    };
    let prepared = target
        .app
        .update(cx, |app, cx| {
            app.prepare_brain_history(&token, &agent, last, grep, cx)
        })
        .map_err(|_| "目标 Workspace 窗口已关闭".to_string())
        .and_then(|result| result.map_err(|error| error.to_string()));
    let work = match prepared {
        Ok(work) => work,
        Err(message) => {
            let _ = response.send(failure(request_id, message));
            return;
        }
    };
    cx.spawn(async move |_| {
        let reply = match work.await {
            Ok(output) => WireResponse::new(request_id, Ok(CardCommandResult::Brain { output })),
            Err(message) => failure(request_id, message),
        };
        let _ = response.send(reply);
    })
    .detach();
}
