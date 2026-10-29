//! Workspace-scoped content tab persistence and selection.

use super::super::content_node_id;
use crate::app::{AppScreen, MultisoulApp};
use agent::AdapterError;
use gpui::Context;
use storage::{DesktopContentKind, NewDesktopContentTab};

impl MultisoulApp {
    pub(crate) fn norma_open_desktop_content_tab(
        &mut self,
        kind: DesktopContentKind,
        resource: String,
        title: String,
        cx: &mut Context<Self>,
    ) -> Result<(), AdapterError> {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            let source_entity = cx.entity_id();
            return host.update(cx, |host, cx| {
                if host.screen != AppScreen::DesktopWorkspace
                    || host.desktop_workspace.active_id.is_none()
                {
                    return Err(AdapterError::Rejected(
                        ui::i18n::text("drawers.desktop_closed").into(),
                    ));
                }
                let source_area = host.desktop_workspace.sessions.iter().find_map(
                    |((workspace_id, area_id), session)| {
                        (host.desktop_workspace.active_id.as_deref() == Some(workspace_id.as_str())
                            && session.entity_id() == source_entity)
                            .then(|| area_id.clone())
                    },
                );
                let source_area = source_area.ok_or_else(|| {
                    AdapterError::Rejected(ui::i18n::text("drawers.source_project_changed").into())
                })?;
                host.open_desktop_content_tab(kind, resource, title, Some(source_area), cx);
                Ok(())
            });
        }
        if self.screen != AppScreen::DesktopWorkspace || self.desktop_workspace.active_id.is_none()
        {
            return Err(AdapterError::Rejected(
                ui::i18n::text("drawers.no_workspace").into(),
            ));
        }
        self.open_desktop_content_tab(
            kind,
            resource,
            title,
            self.desktop_workspace.expanded_area_id.clone(),
            cx,
        );
        Ok(())
    }

    pub(crate) fn route_project_content_to_desktop(
        &mut self,
        kind: DesktopContentKind,
        resource: String,
        title: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) else {
            return false;
        };
        let source_entity = cx.entity_id();
        let routed = host.update(cx, |host, cx| {
            let source_area = host.desktop_workspace.sessions.iter().find_map(
                |((workspace_id, area_id), session)| {
                    (host.desktop_workspace.active_id.as_deref() == Some(workspace_id.as_str())
                        && session.entity_id() == source_entity)
                        .then(|| area_id.clone())
                },
            );
            let Some(source_area) = source_area else {
                return false;
            };
            host.open_desktop_content_tab(kind, resource, title, Some(source_area), cx);
            true
        });
        if !routed {
            self.workspace_state.error = Some(
                ui::i18n::LocalizedText::new("common.operation_failed")
                    .with_arg("reason", ui::i18n::text("drawers.return_workspace")),
            );
            cx.notify();
        }
        true
    }

    pub(crate) fn open_desktop_content_tab(
        &mut self,
        kind: DesktopContentKind,
        resource: String,
        title: String,
        source_area_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.open_desktop_content_tab_with_version(kind, resource, title, source_area_id, None, cx);
    }

    pub(crate) fn open_desktop_content_tab_with_version(
        &mut self,
        kind: DesktopContentKind,
        resource: String,
        title: String,
        source_area_id: Option<String>,
        version: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let source_project_name = source_area_id
            .as_ref()
            .and_then(|id| {
                self.desktop_workspace
                    .areas
                    .iter()
                    .find(|area| &area.id == id)
            })
            .map(|area| area.display_name.clone())
            .unwrap_or_else(|| {
                self.desktop_workspace
                    .active()
                    .map(|workspace| workspace.name.clone())
                    .unwrap_or_default()
            });
        let request = NewDesktopContentTab {
            workspace_id: workspace_id.clone(),
            source_area_id,
            source_project_name,
            kind,
            resource,
            version,
            title,
        };
        self.enqueue_desktop_content_open(request, false, None, None, cx);
    }

    pub(crate) fn hide_desktop_content_browsers(&mut self) {
        for tab in &self.desktop_workspace.content_tabs {
            self.browsers.hide(content_node_id(&tab.id));
        }
    }

    pub(crate) fn update_desktop_browser_tab_location(
        &mut self,
        node_id: canvas::NodeId,
        resource: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return false;
        };
        let resource = crate::app::browser::canonical_browser_location(&resource);
        let Some(tab) = self.desktop_workspace.content_tabs.iter_mut().find(|tab| {
            tab.kind == DesktopContentKind::Browser && content_node_id(&tab.id) == node_id
        }) else {
            return false;
        };
        if tab.resource == resource {
            return false;
        }
        let title = url::Url::parse(&resource)
            .ok()
            .and_then(|url| url.host_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| ui::i18n::text("drawers.browser").to_string());
        tab.resource = resource.clone();
        tab.title = title.clone();
        let tab_id = tab.id.clone();
        let store =
            storage::DesktopContentTabStore::new(self.database_paths.app_db_path().to_owned());
        self.desktop_workspace.content_tab_generation = self
            .desktop_workspace
            .content_tab_generation
            .wrapping_add(1);
        let generation = self.desktop_workspace.content_tab_generation;
        let expected_workspace_id = workspace_id.clone();
        let previous = self.desktop_workspace.content_tab_task.take();
        self.desktop_workspace.content_tab_task = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let result = cx
                .background_executor()
                .spawn(async move {
                    let update =
                        store.update_browser_location(&workspace_id, &tab_id, &resource, &title)?;
                    Ok::<_, storage::StorageError>((update, store.list(&workspace_id)?))
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.active_id.as_deref()
                    != Some(expected_workspace_id.as_str())
                {
                    return;
                }
                match result {
                    Ok((update, tabs)) => {
                        if let Some(removed_id) = update.removed_tab_id {
                            app.browsers.remove(content_node_id(&removed_id));
                        }
                        if app.desktop_workspace.content_tab_generation == generation {
                            app.desktop_workspace.content_tabs = tabs;
                        }
                    }
                    Err(error) => {
                        app.desktop_workspace.error = Some(ui::i18n::message(
                            "drawers.save_url_failed",
                            &[("error", (error).to_string())],
                        ));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
        true
    }

    pub(super) fn select_desktop_content_tab(&mut self, tab_id: &str, cx: &mut Context<Self>) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let Some(tab) = self
            .desktop_workspace
            .content_tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .cloned()
        else {
            return;
        };
        self.hide_desktop_content_browsers();
        let store =
            storage::DesktopContentTabStore::new(self.database_paths.app_db_path().to_owned());
        let tab_id = tab_id.to_owned();
        let expected_workspace_id = workspace_id.clone();
        self.desktop_workspace.content_tab_generation = self
            .desktop_workspace
            .content_tab_generation
            .wrapping_add(1);
        let previous = self.desktop_workspace.content_tab_task.take();
        self.desktop_workspace.content_tab_task = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let result = cx
                .background_executor()
                .spawn(async move {
                    if tab.kind == DesktopContentKind::Browser {
                        store.select_or_matching_browser(
                            &workspace_id,
                            &tab_id,
                            &tab.resource,
                            tab.version.as_deref(),
                            &tab.instance_id,
                        )?;
                    } else {
                        store.select(&workspace_id, &tab_id)?;
                    }
                    store.list(&workspace_id)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.active_id.as_deref()
                    != Some(expected_workspace_id.as_str())
                {
                    return;
                }
                if let Ok(tabs) = result {
                    app.desktop_workspace.content_tabs = tabs;
                    app.load_selected_desktop_content(cx);
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn close_desktop_content_tab(&mut self, tab_id: &str, cx: &mut Context<Self>) {
        if self
            .desktop_workspace
            .content_tabs
            .iter()
            .any(|t| t.id == tab_id && t.kind == DesktopContentKind::ProjectFiles)
            && self.desktop_workspace.workbench.has_edits()
        {
            self.desktop_workspace.workbench.pending_close = Some(
                super::workbench::PendingWorkbenchAction::CloseTab(tab_id.to_string()),
            );
            self.hide_desktop_content_browsers();
            cx.notify();
            return;
        }
        if self
            .desktop_workspace
            .content_tabs
            .iter()
            .any(|t| t.id == tab_id && t.kind == DesktopContentKind::ProjectFiles)
        {
            let saved = std::mem::take(&mut self.desktop_workspace.workbench.saved);
            let persistence = self.desktop_workspace.workbench.persistence.take();
            self.desktop_workspace.workbench = Default::default();
            self.desktop_workspace.workbench.saved = saved;
            self.desktop_workspace.workbench.persistence = persistence;
        }
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let store =
            storage::DesktopContentTabStore::new(self.database_paths.app_db_path().to_owned());
        let tab_id = tab_id.to_owned();
        let expected_workspace_id = workspace_id.clone();
        self.browsers.remove(content_node_id(&tab_id));
        self.desktop_workspace.content_previews.remove(&tab_id);
        self.desktop_workspace.content_snapshots.remove(&tab_id);
        self.desktop_workspace.content_resources.remove(&tab_id);
        self.desktop_workspace.content_loading.remove(&tab_id);
        self.desktop_workspace.content_load_versions.remove(&tab_id);
        self.desktop_workspace.content_tab_generation = self
            .desktop_workspace
            .content_tab_generation
            .wrapping_add(1);
        let previous = self.desktop_workspace.content_tab_task.take();
        self.desktop_workspace.content_tab_task = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let result = cx
                .background_executor()
                .spawn(async move {
                    store.close(&workspace_id, &tab_id)?;
                    store.list(&workspace_id)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.active_id.as_deref()
                    != Some(expected_workspace_id.as_str())
                {
                    return;
                }
                if let Ok(tabs) = result {
                    app.desktop_workspace.content_tabs = tabs;
                    app.load_selected_desktop_content(cx);
                    cx.notify();
                }
            });
        }));
    }
}
