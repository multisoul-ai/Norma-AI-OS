use super::NormaEndpointHost;
use crate::mobile::{LiveCommandRequest, MobileCommand};
use gpui::App;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use storage::{MobileTaskBaselineStore, MobileTurnKey, TaskBaseline};

#[cfg(test)]
#[path = "../../../tests/unit/mobile_task_baseline_tests.rs"]
mod tests;

/// Capture before any Provider input. The existing dispatch path still owns final card/lease checks.
pub(super) fn prepare(database: PathBuf, call: LiveCommandRequest, cx: &mut App) {
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
    let root = target.as_ref().and_then(|target| {
        target
            .window
            .update(cx, |_, _, cx| {
                target.app.update(cx, |app, _| {
                    if app.database_paths.app_db_path() != database
                        || app.workspace_id.as_deref() != Some(&call.request.workspace_id)
                        || app.workspace_route_id.as_deref() != Some(&call.request.route_id)
                    {
                        return None;
                    }
                    app.runtime.workspace_root().map(Path::to_path_buf)
                })
            })
            .ok()
            .and_then(Result::ok)
            .flatten()
    });
    let Some(root) = root else {
        let _ = call.reply.try_send(Err("route_expired"));
        return;
    };
    let Some(expected) = call.request.command.message_target() else {
        let _ = call.reply.try_send(Err("invalid_request"));
        return;
    };
    let expected = expected.clone();
    //10s capture + existing25s Provider acknowledgement +5s database margin fits the40s send budget.
    let capture_deadline = call.deadline.min(Instant::now() + Duration::from_secs(10));
    let feedback = match &call.request.command {
        MobileCommand::FeedbackDelivery { feedback, .. } => Some(feedback.clone()),
        _ => None,
    };
    let feedback_store = storage::MobileCommandStore::new(database.clone());
    let feedback_source = call.source.clone();
    let capture = cx.background_executor().spawn(async move {
        if let Some(feedback) = feedback {
            feedback_store
                .prepare_delivery_feedback(&feedback_source, &feedback)
                .map_err(|error| match error {
                    storage::StorageError::InvalidMobileCommand(code)
                        if code == "delivery_missing" =>
                    {
                        "delivery_missing"
                    }
                    storage::StorageError::InvalidMobileCommand(code)
                        if code == "delivery_changed" =>
                    {
                        "delivery_changed"
                    }
                    storage::StorageError::InvalidMobileCommand(code)
                        if code == "delivery_not_ready" =>
                    {
                        "delivery_not_ready"
                    }
                    _ => "feedback_not_saved",
                })?;
        }
        Ok(match workspace::review::capture(&root, capture_deadline) {
            Ok(review) => TaskBaseline::Available { review },
            Err(reason) => TaskBaseline::Unavailable { reason },
        })
    });
    cx.spawn(async move |cx| {
        let baseline = match capture.await {
            Ok(baseline) => baseline,
            Err(code) => { let _ = call.reply.try_send(Err(code)); return; }
        };
        let source = call.source.clone();
        let deadline = call.deadline;
        let reply = call.reply.clone();
        let (sent, received) = crossbeam_channel::bounded(1);
        let path = database.clone();
        // Do not claim the lease while capturing: revocation, another phone or native input can win.
        if cx.update(|cx| super::mobile_command::execute_now(database, LiveCommandRequest { reply: sent, ..call }, cx)).is_err() {
            let _ = reply.try_send(Err("route_expired")); return;
        }
        // The Provider reply itself depends on foreground progress. Never block an executor worker
        // waiting for it: TestAppContext can run background jobs on its deterministic main executor.
        let result = loop {
            match received.try_recv() {
                Ok(result) => break result,
                Err(crossbeam_channel::TryRecvError::Disconnected) => break Err("outcome_unknown"),
                Err(crossbeam_channel::TryRecvError::Empty) => {},
            }
            if Instant::now() >= deadline { break Err("outcome_unknown"); }
            //25ms matches the existing Provider acknowledgement loop, only while this send is pending.
            cx.background_executor().timer(Duration::from_millis(25)).await;
        };
        let key = result.as_ref().ok().and_then(|value| confirmed_turn(value, &expected).ok());
        let origin = source.clone();
        let turn = key.clone();
        cx.background_executor().spawn(async move {
            if let Some(key) = turn {
                match MobileTaskBaselineStore::new(path).record(&origin, &key, &baseline) {
                    Ok(_) => {},
                    // A missing artifact record does not erase a real Provider sending receipt.
                    // The delivery endpoint must report missing baseline; never reconstruct it from later files.
                    Err(error) => tracing::warn!(%error, operation_id=%origin.operation_id, "mobile task baseline could not be saved"),
                }
            }
        }).await;
        if let Some(target) = target {
            let _ = target.app.update(cx, |app, cx| {
                if let Some(key) = key { app.watch_mobile_delivery(key, source, cx); }
                else { app.clear_mobile_delivery(expected.node_id, &source, cx); }
            });
        }
        let _ = reply.try_send(result);
    }).detach();
}

fn confirmed_turn(
    value: &serde_json::Value,
    expected: &norma::mobile::CardSnapshot,
) -> Result<MobileTurnKey, String> {
    let key = MobileTurnKey {
        node_id: value["node_id"].as_u64().ok_or("missing node")?,
        profile: expected.profile.clone(),
        run_id: value["run_id"].as_str().ok_or("missing run")?.into(),
        session_id: value["session_id"]
            .as_str()
            .ok_or("missing session")?
            .into(),
        turn_id: value["turn_id"].as_str().ok_or("missing turn")?.into(),
    };
    key.validate().map_err(|e| e.to_string())?;
    if key.node_id != expected.node_id
        || expected.run_id.as_ref().is_some_and(|r| r != &key.run_id)
        || expected.turn_id.as_ref().is_some_and(|t| t == &key.turn_id)
        || expected
            .session_id
            .as_ref()
            .is_some_and(|s| s != &key.session_id)
    {
        return Err("confirmed task identity mismatch".into());
    }
    Ok(key)
}
