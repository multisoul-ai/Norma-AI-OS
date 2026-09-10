use super::NormaEndpointHost;
use crate::mobile::LiveConversationRequest;
use gpui::App;
use norma::mobile::{ConversationPage, ConversationRequest};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

pub(crate) fn read(database: PathBuf, call: LiveConversationRequest, cx: &mut App) {
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
    let prepared = target
        .window
        .update(cx, |_, _, cx| {
            target.app.update(cx, |app, cx| {
                if !matches(app, &database, &call.request, call.archive.is_some()) {
                    return Err("session_changed");
                }
                if Instant::now() >= call.deadline {
                    return Err("unavailable");
                }
                if let Some(key) = call.archive.clone() {
                    let reader = app
                        .runtime
                        .mobile_archive_read()
                        .map_err(|_| "unavailable")?;
                    let store = storage::MobileTurnStore::for_workspace(
                        database.clone(),
                        call.request.workspace_id.clone(),
                    );
                    let cursor = call.request.cursor.clone();
                    let deadline = call.deadline;
                    return Ok(cx.background_executor().spawn(async move {
                        let saved = store
                            .session(&key)
                            .map_err(|_| "history_unavailable")?
                            .ok_or("history_unavailable")?;
                        reader
                            .read(saved, cursor, deadline)
                            .map_err(|_| "history_unavailable")
                    }));
                }
                let card = app
                    .runtime
                    .mobile_card_snapshots()
                    .into_iter()
                    .find(|c| c.node_id == call.request.node_id)
                    .ok_or("not_found")?;
                if let Some((reader, saved)) = app
                    .runtime
                    .mobile_native_history(card.node_id)
                    .map_err(|_| "history_unavailable")?
                {
                    let cursor = call.request.cursor.clone();
                    let deadline = call.deadline;
                    return Ok(cx.background_executor().spawn(async move {
                        reader
                            .read(saved, cursor, deadline)
                            .map_err(|_| "history_unavailable")
                    }));
                }
                match card.profile.as_str() {
                    "Codex" => {
                        let cursor = call
                            .request
                            .cursor
                            .as_ref()
                            .map(|c| serde_json::from_str::<String>(c))
                            .transpose()
                            .map_err(|_| "invalid_cursor")?;
                        let receiver = app
                            .runtime
                            .mobile_codex_history(
                                call.request.node_id,
                                &call.request.session_id,
                                cursor,
                            )
                            .map_err(|_| "history_unavailable")?;
                        let deadline = call.deadline;
                        Ok(cx.background_executor().spawn(async move {
                            let raw = receiver
                                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                                .map_err(|_| "unavailable")?
                                .map_err(|_| "history_unavailable")?;
                            ConversationPage::from_codex(raw).map_err(|_| "invalid_history")
                        }))
                    }
                    "Claude" => {
                        let cursor = call
                            .request
                            .cursor
                            .as_ref()
                            .map(|c| serde_json::from_str::<norma::claude::TranscriptCursor>(c))
                            .transpose()
                            .map_err(|_| "invalid_cursor")?;
                        let request = app
                            .runtime
                            .mobile_claude_history(call.request.node_id, &call.request.run_id)
                            .map_err(|_| "history_unavailable")?;
                        Ok(cx.background_executor().spawn(async move {
                            let raw = request
                                .read(cursor, 20)
                                .map_err(|_| "history_unavailable")?;
                            ConversationPage::from_claude(raw).map_err(|_| "invalid_history")
                        }))
                    }
                    _ => Err("history_unavailable"),
                }
            })
        })
        .ok()
        .and_then(Result::ok)
        .unwrap_or(Err("route_expired"));
    let work = match prepared {
        Ok(work) => work,
        Err(code) => {
            let _ = call.reply.try_send(Err(code));
            return;
        }
    };
    cx.spawn(async move |cx| {
        let page = work.await;
        let current = target
            .window
            .update(cx, |_, _, cx| {
                target.app.update(cx, |app, _| {
                    matches(app, &database, &call.request, call.archive.is_some())
                })
            })
            .ok()
            .and_then(Result::ok)
            .unwrap_or(false);
        let outcome = if !current {
            Err("session_changed")
        } else if Instant::now() >= call.deadline {
            Err("unavailable")
        } else {
            page
        };
        let _ = call.reply.try_send(outcome);
    })
    .detach();
}

fn matches(
    app: &crate::app::MultisoulApp,
    database: &Path,
    request: &ConversationRequest,
    archive: bool,
) -> bool {
    app.database_paths.app_db_path() == database
        && app.workspace_id.as_deref() == Some(&request.workspace_id)
        && app.workspace_route_id.as_deref() == Some(&request.route_id)
        && (archive
            || (app.runtime.mobile_agent_run_id(request.node_id) == Some(request.run_id.as_str())
                && app.runtime.agent_session_id(request.node_id).as_deref()
                    == Some(&request.session_id)))
}
