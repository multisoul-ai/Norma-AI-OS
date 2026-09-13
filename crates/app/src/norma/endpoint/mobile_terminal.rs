use super::NormaEndpointHost;
use crate::mobile::LiveTerminalRequest;
use gpui::App;
use std::{path::Path, time::Instant};

pub(crate) fn read(database: &Path, call: LiveTerminalRequest, cx: &mut App) {
    let target = cx
        .try_global::<NormaEndpointHost>()
        .and_then(|host| {
            host.routes
                .resolve(&call.request.workspace_id, &call.request.route_id)
                .ok()
        })
        .cloned();
    let result = target
        .and_then(|target| {
            target
                .app
                .update(cx, |app, _| {
                    if app.database_paths.app_db_path() != database
                        || app.workspace_id.as_deref() != Some(&call.request.workspace_id)
                        || app.workspace_route_id.as_deref() != Some(&call.request.route_id)
                    {
                        return Err("route_expired");
                    }
                    if Instant::now() >= call.deadline {
                        return Err("unavailable");
                    }
                    app.runtime.mobile_terminal_page(&call.request)
                })
                .ok()
        })
        .unwrap_or(Err("route_expired"));
    let _ = call.reply.try_send(result);
}
