//! Present content to the desktop drawer, acknowledging only a visible installation.
use super::{resolve_targets, routing_error_response, validate_source, WorkspaceTarget};
use crate::app::{AppScreen, MultisoulApp};
use gpui::App;
use norma::protocol::{
    CardCommand, CardCommandError, CardCommandResult, PresentationErrorCode, PresentationTarget,
    WireRequest, WireResponse,
};
use std::time::Instant;

fn failure(message: impl Into<String>) -> CardCommandError {
    CardCommandError::presentation(PresentationErrorCode::PresentationFailed, message, false)
}

pub(crate) struct DesktopPresentationReceipt {
    source: WorkspaceTarget,
    request: WireRequest,
    response: Option<crossbeam_channel::Sender<WireResponse>>,
    started: Instant,
    pub(crate) source_name: String,
}

impl DesktopPresentationReceipt {
    pub(crate) fn validate(
        &self,
        host: &MultisoulApp,
        cx: &mut App,
    ) -> Result<(), CardCommandError> {
        let routed = resolve_targets(&self.request, cx).map_err(|_| failure("来源项目已关闭"))?;
        if routed.source.app.entity_id() != self.source.app.entity_id()
            || host.screen != AppScreen::DesktopWorkspace
            || !host
                .desktop_workspace
                .sessions
                .iter()
                .any(|((workspace, _), session)| {
                    host.desktop_workspace.active_id.as_deref() == Some(workspace.as_str())
                        && session.entity_id() == self.source.app.entity_id()
                })
        {
            return Err(failure("来源项目已切换，内容未展示"));
        }
        validate_source(&self.source, &self.request, cx).map_err(|reply| {
            reply
                .result
                .err()
                .unwrap_or_else(|| failure("来源身份失效"))
        })?;
        self.source
            .app
            .update(cx, |app, cx| {
                if app.screen != AppScreen::Canvas
                    || app.runtime.workspace_root().is_none()
                    || !app
                        .project_window
                        .is_some_and(|window| cx.windows().contains(&window))
                {
                    return Err(failure("内容展示需要可见的项目窗口"));
                }
                Ok(())
            })
            .map_err(|_| failure("来源项目已释放"))?
    }

    pub(crate) fn focus(&self, cx: &mut App) -> Result<(), CardCommandError> {
        self.source
            .with_optional_window(cx, |window, cx| {
                let window = window.ok_or_else(|| failure("展示窗口已关闭"))?;
                self.source
                    .app
                    .update(cx, |app, _| {
                        app.release_all_browser_input();
                        app.focus_handle.focus(window);
                    })
                    .map_err(|_| failure("来源项目已释放"))
            })
            .map_err(|_| failure("展示窗口已关闭"))?
    }

    pub(crate) fn finish(mut self, result: Result<CardCommandResult, CardCommandError>) {
        self.send(result);
    }

    fn send(&mut self, result: Result<CardCommandResult, CardCommandError>) {
        if let Some(response) = self.response.take() {
            crate::norma::server::log_managed_request(
                self.request.request_id,
                &self.request.command,
                self.request.authorization.as_ref(),
                self.request.source_node_id,
                &result,
                self.started.elapsed(),
            );
            let _ = response.send(WireResponse::new(self.request.request_id, result));
        }
    }
}

impl Drop for DesktopPresentationReceipt {
    fn drop(&mut self) {
        self.send(Err(failure("展示请求已取消，内容未展示")));
    }
}

pub(crate) fn dispatch(
    request: WireRequest,
    response: crossbeam_channel::Sender<WireResponse>,
    cx: &mut App,
) {
    let routed = match resolve_targets(&request, cx) {
        Ok(routed) => routed,
        Err(error) => {
            let _ = response.send(routing_error_response(request.request_id, error));
            return;
        }
    };
    if routed.source.summary.workspace_id != routed.target.summary.workspace_id {
        let _ = response.send(WireResponse::new(
            request.request_id,
            Err(failure("Present 暂不支持跨项目目标")),
        ));
        return;
    }
    let source = match validate_source(&routed.source, &request, cx) {
        Ok(source) => source.source,
        Err(reply) => {
            let _ = response.send(*reply);
            return;
        }
    };
    let context = routed.source.app.update(cx, |app, cx| {
        let host = app.desktop_host.as_ref().and_then(|host| host.upgrade());
        let source_name = source
            .definition_id
            .and_then(|id| {
                app.agent_definitions
                    .definitions()
                    .iter()
                    .find(|definition| definition.id == id)
                    .map(|definition| definition.name.clone())
            })
            .unwrap_or_else(|| "Agent".into());
        (host, source_name, cx.entity_id())
    });
    let Ok((Some(host), source_name, source_entity)) = context else {
        let _ = response.send(super::execute_in_workspace(routed.target, request, cx));
        return;
    };
    let receipt = DesktopPresentationReceipt {
        source: routed.source,
        request: request.clone(),
        response: Some(response),
        started: Instant::now(),
        source_name,
    };
    let CardCommand::Present { target, create_new } = request.command else {
        return;
    };
    let mut receipt = Some(receipt);
    // Reserve the content queue position in the request's foreground turn. Preparing a file
    // before enqueueing can otherwise overtake a later Present or user tab close.
    let result = host.update(cx, |host, cx| {
        receipt
            .as_ref()
            .expect("pending receipt")
            .validate(host, cx)?;
        let workspace_id = host
            .desktop_workspace
            .active_id
            .clone()
            .ok_or_else(|| failure("桌面工作空间已关闭"))?;
        let area_id = host
            .desktop_workspace
            .sessions
            .iter()
            .find_map(|((workspace, area), session)| {
                (workspace == &workspace_id && session.entity_id() == source_entity)
                    .then(|| area.clone())
            })
            .ok_or_else(|| failure("来源项目已移除"))?;
        let source_project_name = host
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == area_id)
            .map(|area| area.display_name.clone())
            .ok_or_else(|| failure("来源项目已移除"))?;
        let kind = match &target {
            PresentationTarget::Browser { .. } => storage::DesktopContentKind::Browser,
            PresentationTarget::Preview { .. } => storage::DesktopContentKind::File,
            PresentationTarget::Ego => return Err(failure("Ego 使用独立展示路径")),
        };
        host.enqueue_desktop_content_open(
            storage::NewDesktopContentTab {
                workspace_id,
                source_area_id: Some(area_id),
                source_project_name,
                kind,
                resource: String::new(),
                version: None,
                title: String::new(),
            },
            create_new,
            Some(target),
            receipt.take(),
            cx,
        );
        Ok(())
    });
    if let (Err(error), Some(receipt)) = (result, receipt) {
        receipt.finish(Err(error));
    }
}
