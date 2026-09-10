use super::NormaEndpointHost;
use crate::mobile::{LiveReviewRequest, ProjectReviewRequest};
use gpui::App;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

pub(crate) fn read(database: PathBuf, call: LiveReviewRequest, cx: &mut App) {
    if call.request.delivery_key.is_some() {
        let _ = call.reply.try_send(Err("invalid_request"));
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
    let root = target
        .window
        .update(cx, |_, _, cx| {
            target.app.update(cx, |app, _| {
                if !matches(app, &database, &call.request) || Instant::now() >= call.deadline {
                    return None;
                }
                app.runtime.workspace_root().map(Path::to_path_buf)
            })
        })
        .ok()
        .and_then(Result::ok)
        .flatten();
    let Some(root) = root else {
        let _ = call.reply.try_send(Err("route_expired"));
        return;
    };
    let deadline = call.deadline;
    let work = cx.background_executor().spawn(async move {
        workspace::review::capture(&root, deadline).map_err(|error| match error {
            workspace::review::ReviewError::Expired => "review_expired",
            workspace::review::ReviewError::TooLarge => "review_too_large",
            workspace::review::ReviewError::Changed => "review_changed",
            workspace::review::ReviewError::Unsupported => "review_unsupported",
            _ => "review_unavailable",
        })
    });
    cx.spawn(async move |cx| {
        let result = work.await;
        let current = target
            .window
            .update(cx, |_, _, cx| {
                target
                    .app
                    .update(cx, |app, _| matches(app, &database, &call.request))
            })
            .ok()
            .and_then(Result::ok)
            .unwrap_or(false);
        let result = if !current {
            Err("route_expired")
        } else if Instant::now() >= deadline {
            Err("review_expired")
        } else {
            result
        };
        let _ = call.reply.try_send(result);
    })
    .detach();
}

fn matches(
    app: &crate::app::MultisoulApp,
    database: &Path,
    request: &ProjectReviewRequest,
) -> bool {
    request.valid()
        && app.database_paths.app_db_path() == database
        && app.workspace_id.as_deref() == Some(&request.workspace_id)
        && app.workspace_route_id.as_deref() == Some(&request.route_id)
}
