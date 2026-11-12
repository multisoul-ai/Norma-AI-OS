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
    let CardCommand::Brain { token, op } = request.command.clone() else {
        return;
    };
    let BrainOp::Task { op } = *op else {
        return;
    };
    let target = match resolve_targets(&request, cx) {
        Ok(routed) => routed.target,
        Err(error) => {
            let _ = response.send(routing_error_response(id, error));
            return;
        }
    };
    let work = target
        .app
        .update(cx, |app, cx| app.start_staff_command(&token, op, cx))
        .map_err(|_| ui::i18n::text("staff.invalid_source").to_owned())
        .and_then(|result| result);
    cx.spawn(async move |_| {
        let result = match work {
            Ok(work) => work.await,
            Err(error) => Err(error),
        };
        let result = result
            .map(|output| CardCommandResult::Brain { output })
            .map_err(|message| CardCommandError::Unsupported {
                message: crate::app::staff::error_message(message).to_string(),
            });
        let _ = response.send(WireResponse::new(id, result));
    })
    .detach();
}
