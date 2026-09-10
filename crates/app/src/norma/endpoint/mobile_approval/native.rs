use crate::app::MultisoulApp;
use gpui::Context;
use norma::{agent_hook::ApprovalDecision, mobile::ApprovalTarget};
use serde_json::Value;

/// The gateway has already persisted this command's exact target and decision. Keep the
/// original native CLI; only its own menu shortcut can resolve its foreground permission.
pub(super) fn choose(
    app: &mut MultisoulApp,
    expected: &ApprovalTarget,
    decision: ApprovalDecision,
    cx: &mut Context<MultisoulApp>,
) -> Result<async_channel::Receiver<Result<Value, String>>, &'static str> {
    if app.agent_hook_server.has_pending() {
        return Err("state_not_ready");
    }
    let saved = app
        .runtime
        .mobile_native_session(expected.node_id)
        .map_err(|_| "approval_unavailable")?
        .ok_or("approval_unavailable")?;
    let store = storage::MobileTurnStore::for_workspace(
        app.database_paths.app_db_path().to_path_buf(),
        app.workspace_id.clone().ok_or("approval_unavailable")?,
    );
    let mut request = app.runtime.reserve_native_approval(expected, decision)?;
    let target = expected.clone();
    let preparation = cx.background_executor().spawn(async move {
        let result = request.prepare();
        (request, result)
    });
    let (sender, receipt) = async_channel::bounded(1);
    cx.spawn(async move |this, cx| {
        let result = async {
            let (mut request, prepared) = preparation.await;
            prepared?;
            this.update(cx, |app, cx| {
                if app.agent_hook_server.has_pending() {
                    return Err("approval_changed".to_string());
                }
                app.runtime.send_native_approval(&mut request)?;
                cx.notify();
                Ok(())
            })
            .map_err(|_| "route_expired".to_string())??;
            loop {
                // 100ms = at most ten bounded reads/second while this one command is outstanding.
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let (next, confirmed) = cx
                    .background_executor()
                    .spawn(async move {
                        let result = request.poll();
                        (request, result)
                    })
                    .await;
                request = next;
                if confirmed? {
                    break;
                }
            }
            let result = request.result()?;
            let ended = (decision == ApprovalDecision::Deny).then(|| storage::MobileTurnOutcome {
                key: storage::MobileTurnKey {
                    node_id: target.node_id,
                    profile: target.profile.clone(),
                    run_id: target.run_id.clone(),
                    session_id: target.session_id.clone(),
                    turn_id: target.turn_id.clone(),
                },
                state: storage::MobileTurnState::Interrupted,
            });
            // A native rejection has no Stop Hook. Persist its verified turn boundary before
            // publishing success, using the same store as the original card's native stop.
            cx.background_executor()
                .spawn(async move {
                    store.record_native_event(
                        &saved.session,
                        saved.transcript_path.as_deref(),
                        ended.as_ref(),
                    )
                })
                .await
                .map_err(|_| "outcome_unknown".to_string())?;
            let finished = this
                .update(cx, |app, cx| {
                    if app.agent_hook_server.has_pending() {
                        return false;
                    }
                    let finished = app.runtime.finish_native_approval(&request);
                    cx.notify();
                    finished
                })
                .map_err(|_| "route_expired".to_string())?;
            if !finished {
                return Err("outcome_unknown".into());
            }
            crate::mobile::project_events::changed();
            Ok(result)
        }
        .await;
        let _ = sender.try_send(result);
    })
    .detach();
    cx.notify();
    Ok(receipt)
}
