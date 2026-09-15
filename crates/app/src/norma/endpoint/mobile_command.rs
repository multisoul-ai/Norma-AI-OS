use super::NormaEndpointHost;
use crate::{
    mobile::{LiveCommandRequest, MobileCommand, MobileProvider},
    workspace::persistence::PersistenceSaveState,
};
use gpui::App;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) enum Started {
    Immediate(serde_json::Value),
    Durable(async_channel::Receiver<Result<serde_json::Value, &'static str>>),
    Saved(
        serde_json::Value,
        Option<async_channel::Receiver<Result<(), &'static str>>>,
    ),
    Async(async_channel::Receiver<Result<serde_json::Value, String>>),
}

/// The gateway has already reserved a durable operation and issued a revocable dispatch claim.
pub(crate) fn execute(database: PathBuf, call: LiveCommandRequest, cx: &mut App) {
    if call.request.command.message_target().is_some() {
        super::mobile_task_baseline::prepare(database, call, cx);
    } else {
        execute_now(database, call, cx);
    }
}

pub(super) fn execute_now(database: PathBuf, call: LiveCommandRequest, cx: &mut App) {
    if let Err(code) = call.request.command.validate_feedback_target() {
        let _ = call.reply.try_send(Err(code));
        return;
    }
    let target = cx
        .try_global::<NormaEndpointHost>()
        .and_then(|host| {
            host.routes
                .resolve(&call.request.workspace_id, &call.request.route_id)
                .ok()
        })
        .cloned();
    let Some(target) = target else {
        let _ = call.reply.try_send(Err("route_expired"));
        return;
    };
    let created = target
        .with_optional_window(cx, |mut window, cx| {
            target.app.update(cx, |app, cx| {
                if app.database_paths.app_db_path() != database
                    || app.workspace_id.as_deref() != Some(&call.request.workspace_id)
                    || app.workspace_route_id.as_deref() != Some(&call.request.route_id)
                {
                    return Err("route_expired");
                }
                if app.persistence.project_store().is_none() {
                    return Err("unavailable");
                }
                if Instant::now() >= call.deadline {
                    return Err("expired");
                }
                if !call.lease.begin() {
                    return Err("unauthorized");
                }
                match &call.request.command {
                    MobileCommand::RealtimeTool { command } => {
                        super::mobile_realtime::start(app, window.as_deref_mut(), cx, command.clone(), call.source.clone())
                    }
                    MobileCommand::TerminalInput { expected, screen_signature, input_sequence, input } => {
                        app.runtime.send_mobile_terminal_input(expected, *screen_signature, *input_sequence, input)?;
                        cx.notify();
                        Ok(Started::Immediate(serde_json::json!({"node_id":expected.node_id,
                            "run_id":expected.run_id,"session_id":expected.session_id,"status":"written"})))
                    }
                    MobileCommand::AcceptDelivery { key, fingerprint } => {
                        if app.mobile_card_snapshots().iter().any(|card| {
                            card.node_id == key.node_id
                                && !matches!(
                                    card.state.as_str(),
                                    "idle" | "ready" | "done" | "ended" | "failed"
                                )
                        }) {
                            return Err("state_not_ready");
                        }
                        let (sender, receiver) = async_channel::bounded(1);
                        let database = database.clone();
                        let source = call.source.clone();
                        let key = key.clone();
                        let fingerprint = fingerprint.clone();
                        cx.background_executor()
                            .spawn(async move {
                                let result = storage::MobileCommandStore::new(database)
                                    .accept_delivery(&source, &key, &fingerprint)
                                    .map(|accepted| accepted.result())
                                    .map_err(|error| match error {
                                        storage::StorageError::InvalidMobileCommand(code) => {
                                            match code.as_str() {
                                                "delivery_missing" => "delivery_missing",
                                                "delivery_changed" => "delivery_changed",
                                                "delivery_not_ready" => "delivery_not_ready",
                                                "delivery_accepted" => "delivery_accepted",
                                                _ => "outcome_unknown",
                                            }
                                        }
                                        _ => "outcome_unknown",
                                    });
                                let _ = sender.try_send(result);
                            })
                            .detach();
                        Ok(Started::Durable(receiver))
                    }
                    MobileCommand::GeneratePlan { .. } => super::mobile_plan::start(
                        app,
                        target.clone(),
                        database.clone(),
                        call.request.clone(),
                        cx,
                    )
                    .map(Started::Durable),
                    MobileCommand::CreateCard {
                        profile,
                        title,
                        definition_id,
                    } => app
                        .create_mobile_card_in_canvas(
                            match profile {
                                MobileProvider::Codex => canvas::AgentProfile::Codex,
                                MobileProvider::Claude => canvas::AgentProfile::Claude,
                                MobileProvider::Cursor => canvas::AgentProfile::Cursor,
                            },
                            title.clone(),
                            *definition_id,
                            window.as_deref_mut(),
                            cx,
                        )
                        .map(|node| Started::Saved(serde_json::json!({"node_id":node}), None))
                        .map_err(|_| "create_failed"),
                    MobileCommand::DeleteCard {
                        expected,
                        confirm_busy,
                    } => {
                        if app.mobile_card_snapshots().iter().any(|card| {
                            card.node_id == expected.node_id && card.state == "collecting"
                        }) {
                            return Err("state_not_ready");
                        }
                        app.delete_mobile_card_in_canvas(expected, *confirm_busy, window, cx)
                            .map(|node| Started::Saved(serde_json::json!({"node_id":node}), None))
                    }
                    MobileCommand::InterruptTurn { expected } => {
                        app.interrupt_mobile_chat(expected, cx).map(Started::Async)
                    }
                    MobileCommand::DecideApproval { expected, decision } => {
                        super::mobile_approval::choose(app, expected, *decision, cx)
                            .map(Started::Async)
                    }
                    MobileCommand::QueueMessage { .. } | MobileCommand::DecidePlanTodo { .. } => {
                        Err("invalid_request")
                    }
                    MobileCommand::SendMessage { expected, .. }
                    | MobileCommand::FeedbackDelivery { expected, .. } => {
                        let text = match &call.request.command {
                            MobileCommand::SendMessage { text, .. } => text.clone(),
                            MobileCommand::FeedbackDelivery { feedback, .. } => feedback.message(),
                            _ => unreachable!("matched message"),
                        };
                        let receipt = app.send_mobile_chat(expected, text, cx)?;
                        app.pending_mobile_deliveries
                            .insert(expected.node_id, call.source.clone());
                        Ok(Started::Async(receipt))
                    }
                }
            })
        })
        .ok()
        .and_then(Result::ok)
        .unwrap_or(Err("route_expired"));
    let (value, saved) = match created {
        Ok(Started::Immediate(value)) => {
            let _ = call.reply.try_send(Ok(value));
            return;
        }
        Ok(Started::Durable(result)) => {
            cx.spawn(async move |_| {
                let outcome = result.recv().await.unwrap_or(Err("outcome_unknown"));
                let _ = call.reply.try_send(outcome);
            })
            .detach();
            return;
        }
        Ok(Started::Saved(value, saved)) => (value, saved),
        Ok(Started::Async(result)) => {
            cx.spawn(async move |_| {
                let result = result
                    .recv()
                    .await
                    .unwrap_or_else(|_| Err("outcome_unknown".into()));
                // Once preparation/input began, an error cannot prove that the Provider received nothing.
                let _ = call.reply.try_send(result.map_err(|_| "outcome_unknown"));
            })
            .detach();
            return;
        }
        Err(code) => {
            let _ = call.reply.try_send(Err(code));
            return;
        }
    };
    // The normal desktop scheduler owns all canvas writes. Observe its completion instead of
    // introducing a concurrent snapshot writer that could overwrite newer desktop changes.
    cx.spawn(async move |cx| {
        let outcome = loop {
            let state = target
                .app
                .update(cx, |app, _| {
                    if app.database_paths.app_db_path() != database
                        || app.workspace_id.as_deref() != Some(&call.request.workspace_id)
                        || app.workspace_route_id.as_deref() != Some(&call.request.route_id)
                    {
                        return None;
                    }
                    Some(app.persistence.save_state())
                })
                .ok()
                .flatten();
            match state {
                Some(PersistenceSaveState::Clean) => {
                    break if let Some(saved) = saved {
                        saved
                            .recv()
                            .await
                            .unwrap_or(Err("outcome_unknown"))
                            .map(|()| value)
                    } else {
                        Ok(value)
                    };
                }
                Some(PersistenceSaveState::Failed) | None => break Err("outcome_unknown"),
                Some(PersistenceSaveState::Pending) => {}
            }
            if Instant::now() >= call.deadline {
                break Err("outcome_unknown");
            }
            // 50ms = 20 cheap memory checks/s, only while this command awaits the 360ms save debounce.
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        };
        let _ = call.reply.try_send(outcome);
    })
    .detach();
}
