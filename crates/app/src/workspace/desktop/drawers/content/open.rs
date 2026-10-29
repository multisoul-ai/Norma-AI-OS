//! Serialized storage and visible installation of content, shared by UI links and Present.
use super::super::content_node_id;
use crate::app::MultisoulApp;
use crate::norma::endpoint::presentation::DesktopPresentationReceipt;
use gpui::Context;
use norma::protocol::{
    CardCommandError, CardCommandResult, PresentationDisposition, PresentationErrorCode,
    PresentationKind, PresentationLayout, PresentationTarget,
};
use runtime::desktop_drawers::{route_drawer, DesktopDrawerEvent};
use storage::{DesktopContentKind, NewDesktopContentTab};

pub(crate) fn presentation_failed(message: impl Into<String>) -> CardCommandError {
    CardCommandError::presentation(PresentationErrorCode::PresentationFailed, message, false)
}

impl MultisoulApp {
    pub(crate) fn reset_desktop_content_tabs(&mut self) {
        self.desktop_workspace.content_workspace_epoch = self
            .desktop_workspace
            .content_workspace_epoch
            .wrapping_add(1);
        self.desktop_workspace.content_tab_generation = self
            .desktop_workspace
            .content_tab_generation
            .wrapping_add(1);
        self.desktop_workspace.content_tabs.clear();
        self.desktop_workspace.content_previews.clear();
        self.desktop_workspace.content_snapshots.clear();
        self.desktop_workspace.content_resources.clear();
        self.desktop_workspace.content_loading.clear();
        self.desktop_workspace.content_load_versions.clear();
    }

    pub(crate) fn enqueue_desktop_content_open(
        &mut self,
        mut request: NewDesktopContentTab,
        create_new: bool,
        target: Option<PresentationTarget>,
        receipt: Option<DesktopPresentationReceipt>,
        cx: &mut Context<Self>,
    ) {
        let workspace_id = request.workspace_id.clone();
        let epoch = self.desktop_workspace.content_workspace_epoch;
        let store =
            storage::DesktopContentTabStore::new(self.database_paths.app_db_path().to_owned());
        self.desktop_workspace.content_tab_generation = self
            .desktop_workspace
            .content_tab_generation
            .wrapping_add(1);
        let generation = self.desktop_workspace.content_tab_generation;
        let previous = self.desktop_workspace.content_tab_task.take();
        self.desktop_workspace.content_tab_task = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let valid = this
                .update(cx, |app, cx| {
                    app.validate_content_open(&workspace_id, epoch, receipt.as_ref(), cx)
                })
                .unwrap_or_else(|_| {
                    Err(presentation_failed(ui::i18n::text("drawers.window_closed")))
                });
            if let Err(error) = valid {
                if let Some(receipt) = receipt {
                    receipt.finish(Err(error));
                }
                return;
            }
            let load_workspace = workspace_id.clone();
            let opened = cx
                .background_executor()
                .spawn(async move {
                    let preview_type = target
                        .map(|target| prepare_content_target(&mut request, target))
                        .transpose()?
                        .flatten();
                    let (tab, created) = store
                        .open_for_presentation(request, create_new, preview_type)
                        .map_err(|error| {
                            presentation_failed(ui::i18n::format(
                                "drawers.save_tab_failed",
                                &[("error", (error).to_string())],
                            ))
                        })?;
                    let tabs = store.list(&load_workspace).map_err(|error| {
                        presentation_failed(ui::i18n::format(
                            "drawers.read_tab_failed",
                            &[("error", (error).to_string())],
                        ))
                    })?;
                    Ok::<_, CardCommandError>((tab, created, tabs))
                })
                .await;
            let result = this
                .update(cx, |app, cx| {
                    app.validate_content_open(&workspace_id, epoch, receipt.as_ref(), cx)?;
                    // A later Present waits for this entire task, including its receipt. Ordinary
                    // navigation retains the existing supersession behavior without silent success.
                    if receipt.is_none()
                        && app.desktop_workspace.content_tab_generation != generation
                    {
                        return Err(presentation_failed(ui::i18n::text(
                            "drawers.open_superseded",
                        )));
                    }
                    let (tab, created, tabs) = opened.map_err(|error| {
                        app.desktop_workspace.error = Some(ui::i18n::message(
                            "drawers.open_content_failed",
                            &[("error", (error).to_string())],
                        ));
                        cx.notify();
                        error
                    })?;
                    app.hide_desktop_content_browsers();
                    // Reused files must refresh both content and type. Invalidate in-flight reads too.
                    if tab.kind == DesktopContentKind::File {
                        app.browsers.remove(content_node_id(&tab.id));
                        app.desktop_workspace.content_previews.remove(&tab.id);
                        app.desktop_workspace.content_resources.remove(&tab.id);
                        app.desktop_workspace.content_snapshots.remove(&tab.id);
                        app.desktop_workspace.content_loading.remove(&tab.id);
                        app.desktop_workspace.content_load_versions.remove(&tab.id);
                    }
                    app.desktop_workspace.content_tabs = tabs;
                    app.desktop_workspace.drawers = route_drawer(
                        app.desktop_workspace.drawers,
                        DesktopDrawerEvent::OpenArtifact,
                    );
                    app.desktop_workspace.drawer_motion.retarget(
                        app.desktop_workspace.drawers.left_open,
                        app.desktop_workspace.drawers.right_open,
                    );
                    app.load_selected_desktop_content(cx);
                    if let Some(receipt) = receipt.as_ref() {
                        receipt.focus(cx)?;
                        app.show_presentation_notification(receipt.source_name.clone(), cx);
                    }
                    app.wake_animation_frame_loop();
                    cx.notify();
                    Ok(CardCommandResult::Presented {
                        node_id: content_node_id(&tab.id),
                        kind: if tab.kind == DesktopContentKind::Browser {
                            PresentationKind::Browser
                        } else {
                            PresentationKind::Preview
                        },
                        disposition: if created {
                            PresentationDisposition::Created
                        } else {
                            PresentationDisposition::Reused
                        },
                        layout: PresentationLayout::ContentDrawer,
                    })
                })
                .unwrap_or_else(|_| {
                    Err(presentation_failed(ui::i18n::text("drawers.window_closed")))
                });
            if let Some(receipt) = receipt {
                receipt.finish(result);
            }
        }));
    }

    fn validate_content_open(
        &self,
        workspace_id: &str,
        epoch: u64,
        receipt: Option<&DesktopPresentationReceipt>,
        cx: &mut Context<Self>,
    ) -> Result<(), CardCommandError> {
        if self.desktop_workspace.active_id.as_deref() != Some(workspace_id)
            || self.desktop_workspace.content_workspace_epoch != epoch
        {
            return Err(presentation_failed(ui::i18n::text(
                "drawers.workspace_changed",
            )));
        }
        if let Some(receipt) = receipt {
            receipt.validate(self, cx)?;
        }
        Ok(())
    }
}

/// Runs on the content queue's background executor before its storage mutation.
fn prepare_content_target(
    request: &mut NewDesktopContentTab,
    target: PresentationTarget,
) -> Result<Option<canvas::PreviewType>, CardCommandError> {
    match norma::presentation::prepare_presentation_target(target)
        .map_err(|error| error.into_card_error())?
    {
        PresentationTarget::Browser { url } => {
            request.title = url::Url::parse(&url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_else(|| ui::i18n::text("drawers.browser").into());
            request.resource = url;
            Ok(None)
        }
        PresentationTarget::Preview {
            path,
            explicit_type,
        } => {
            let kind = crate::preview::infer_preview_type(
                &path,
                explicit_type.map(crate::norma::server::canvas_preview_type),
            )
            .map_err(|_| presentation_failed(ui::i18n::text("drawers.unsupported_preview")))?;
            request.title = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| ui::i18n::text("drawers.preview").into());
            request.resource = path.to_string_lossy().into_owned();
            Ok(Some(kind))
        }
        PresentationTarget::Ego => Err(presentation_failed(ui::i18n::text("drawers.ego_separate"))),
    }
}
