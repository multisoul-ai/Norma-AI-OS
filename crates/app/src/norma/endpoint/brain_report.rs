//! Persist a report and its exact sources before asking the foreground to deliver it.
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
        op: BrainOp::Report {
            text,
            priority,
            sources,
        },
    } = request.command.clone()
    else {
        return;
    };
    let ticket;
    let target = match resolve_targets(&request, cx) {
        Ok(routed) => routed.target,
        Err(error) => {
            let _ = response.send(routing_error_response(id, error));
            return;
        }
    };
    let service = match target.app.update(cx, |app, _| {
        app.prepare_brain_context(&token)
            .map(|service| (service, app.brain_report_ticket()))
    }) {
        Ok(Ok((service, ticket_at_arrival))) => {
            ticket = ticket_at_arrival;
            service
        }
        other => {
            let message = match other {
                Ok(Err(error)) => error.to_string(),
                _ => "目标项目已关闭".into(),
            };
            let _ = response.send(failure(id, message));
            return;
        }
    };
    let text = text.trim().to_owned();
    let report_id = uuid::Uuid::new_v4().to_string();
    cx.spawn(async move |cx| {
        let report_text = text.clone();
        let record = match service
            .run(None, move |access| {
                access.report(&report_id, &report_text, sources.clone())
            })
            .await
        {
            Ok(record) => record,
            Err(error) => {
                let _ = response.send(failure(id, error.to_string()));
                return;
            }
        };
        let reference = record.reference;
        let tracker = match service.report_delivery(reference.clone()) {
            Ok(tracker) => tracker,
            Err(error) => {
                let _ = response.send(failure(id, error.to_string()));
                return;
            }
        };
        let delivery = target
            .app
            .update(cx, |app, cx| {
                if !app
                    .prepare_brain_context(&token)
                    .is_ok_and(|current| current.scope() == service.scope())
                {
                    return None;
                }
                app.deliver_dock_brain_reply(&text, cx);
                // A saved report whose delivery qualification lapsed is Skipped, not an error.
                if !app.brain_report_ticket_valid(ticket) {
                    tracker.cancel();
                    return Some(false);
                }
                Some(app.speak_saved_brain_report(text, priority, tracker.clone(), cx))
            })
            .ok()
            .flatten();
        if delivery.is_none() {
            tracker.cancel();
        }
        let saved = tracker.flush().await;
        let reply = match (delivery, saved) {
            (_, Err(error)) => failure(id, error.to_string()),
            (None, _) => failure(id, "大脑令牌或来源项目已失效".into()),
            (Some(queued), Ok(_)) => WireResponse::new(
                id,
                Ok(CardCommandResult::Brain {
                    output: serde_json::json!({"saved":true,"reference":reference,"queued":queued})
                        .to_string(),
                }),
            ),
        };
        let _ = response.send(reply);
    })
    .detach();
}

fn failure(id: u64, message: String) -> WireResponse {
    WireResponse::new(id, Err(CardCommandError::Unsupported { message }))
}
