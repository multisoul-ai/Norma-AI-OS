use super::NormaEndpointHost;
use crate::mobile::LiveApprovalRequest;
use gpui::App;
use std::{path::Path, time::Instant};
mod native;

pub(crate) fn choose(
    app: &mut crate::app::MultisoulApp,
    expected: &norma::mobile::ApprovalTarget,
    decision: norma::agent_hook::ApprovalDecision,
    cx: &mut gpui::Context<crate::app::MultisoulApp>,
) -> Result<async_channel::Receiver<Result<serde_json::Value, String>>, &'static str> {
    if app
        .runtime
        .mobile_native_session(expected.node_id)
        .map_err(|_| "approval_unavailable")?
        .is_some()
    {
        return native::choose(app, expected, decision, cx);
    }
    let project = app.persistence.project_store().ok_or("unavailable")?;
    let audit = storage::ApprovalAuditStore::new(project.path().to_path_buf());
    app.runtime.choose_mobile_approval(expected, decision)?;
    let expected = expected.clone();
    let (reply, receipt) = async_channel::bounded(1);
    let executor = cx.background_executor().clone();
    cx.background_executor().spawn(async move {
        // 25s leaves 5s of the existing 30s command budget for the final mobile receipt write.
        let deadline = Instant::now() + std::time::Duration::from_secs(25);
        let mut after = 0;
        let result = loop {
            if Instant::now() >= deadline { break Err("outcome_unknown".into()); }
            // Incremental audit cursor: each old entry is read at most once; never scan on GPUI.
            let rows = match audit.list_run(&expected.run_id, after, 256) {
                Ok(rows) => rows,
                Err(_) => break Err("outcome_unknown".into()),
            };
            let more = rows.len() == 256;
            let mut outcome = None;
            for row in rows {
                after = row.sequence;
                if row.request_id != expected.request_id { continue; }
                if row.node_id != expected.node_id || row.provider != expected.profile.to_ascii_lowercase() {
                    outcome = Some(Err("outcome_unknown".into())); break;
                }
                match expected.delivery_state(&row.payload, decision) {
                    Ok("waiting") => {}
                    Ok(status) => {
                        outcome = Some(Ok(serde_json::json!({"node_id":expected.node_id,"run_id":expected.run_id,
                            "session_id":expected.session_id,"turn_id":expected.turn_id,"request_id":expected.request_id,
                            "decision":decision,"status":status})));
                        break;
                    }
                    Err(_) => { outcome = Some(Err("outcome_unknown".into())); break; }
                }
            }
            if let Some(outcome) = outcome { break outcome; }
            if !more { executor.timer(std::time::Duration::from_millis(100)).await; }
        };
        let _ = reply.try_send(result);
    }).detach();
    cx.notify();
    Ok(receipt)
}

pub(crate) fn read(database: &Path, call: LiveApprovalRequest, cx: &mut App) {
    let target = cx
        .try_global::<NormaEndpointHost>()
        .and_then(|host| {
            host.routes
                .resolve(&call.request.workspace_id, &call.request.route_id)
                .ok()
        })
        .cloned();
    let result = target.ok_or("route_expired").and_then(|target| {
        target
            .app
            .update(cx, |app, _| {
                if app.database_paths.app_db_path() != database
                    || app.workspace_id.as_deref() != Some(&call.request.workspace_id)
                    || app.workspace_route_id.as_deref() != Some(&call.request.route_id)
                {
                    return Err("route_expired");
                }
                let cards = app.runtime.mobile_card_snapshots();
                let mut items = Vec::new();
                for card in cards
                    .into_iter()
                    .filter(|c| c.mobile_control && c.run_id.is_some())
                {
                    if Instant::now() >= call.deadline {
                        return Err("unavailable");
                    }
                    items.extend(app.runtime.mobile_approval_snapshots(card.node_id)?);
                    // 128 requests * 64KiB Provider payload = 8MiB maximum foreground handoff.
                    // The background wire layer additionally enforces its existing 500KiB envelope.
                    if items.len() > 128 {
                        return Err("approval_too_large");
                    }
                }
                Ok(items)
            })
            .ok()
            .unwrap_or(Err("route_expired"))
    });
    let _ = call.reply.try_send(result);
}
