//! Workspace-scoped content tab persistence and selection.

use super::super::content_node_id;
use crate::app::{AppScreen, MultisoulApp};
use agent::AdapterError;
use gpui::Context;
use runtime::desktop_drawers::ContentTabCloseScope;
use storage::{DesktopContentKind, NewDesktopContentTab};

#[cfg(test)]
#[path = "../../../../../tests/unit/desktop_artifact_scope_tests.rs"]
mod tests;

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
        self.open_desktop_content_tab(kind, resource, title, self.desktop_content_area_id(), cx);
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
        if tab.resource == resource || self.content_tab_controls.closing.contains(&tab.id) {
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
                            app.desktop_workspace.content_refresh.remove(&removed_id);
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
        if self
            .content_tab_controls
            .closing
            .iter()
            .any(|id| id == tab_id)
        {
            return;
        }
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
        self.desktop_workspace.content_browser.new_tab_page = false;
        let store =
            storage::DesktopContentTabStore::new(self.database_paths.app_db_path().to_owned());
        let tab_id = tab_id.to_owned();
        let expected_workspace_id = workspace_id.clone();
        let generation = self.begin_desktop_content_navigation();
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
                    || app.desktop_workspace.content_tab_generation != generation
                {
                    return;
                }
                if let Ok(tabs) = result {
                    app.desktop_workspace.content_tabs = tabs;
                    app.reveal_committed_content_tab(cx);
                    app.load_selected_desktop_content(cx);
                    cx.notify();
                }
            });
        }));
    }

    pub(super) fn close_desktop_content_tab(&mut self, tab_id: &str, cx: &mut Context<Self>) {
        self.close_desktop_content_tab_scope(tab_id, ContentTabCloseScope::Current, cx);
    }

    pub(super) fn close_desktop_content_tab_scope(
        &mut self,
        target: &str,
        scope: ContentTabCloseScope,
        cx: &mut Context<Self>,
    ) {
        let state = &self.desktop_workspace;
        if state.workbench.pending_close.is_some() || !self.content_tab_controls.closing.is_empty()
        {
            return;
        }
        let ordered = state
            .content_tabs
            .iter()
            .map(|tab| tab.id.as_str())
            .collect::<Vec<_>>();
        let ids = scope.targets(&ordered, target);
        if ids.is_empty() {
            return;
        }
        let closes_workbench = state.content_tabs.iter().any(|tab| {
            tab.kind == DesktopContentKind::ProjectFiles && ids.contains(&tab.id.as_str())
        });
        let ids: Vec<_> = ids.into_iter().map(str::to_owned).collect();
        if closes_workbench && state.workbench.has_edits() {
            self.desktop_workspace.workbench.pending_close =
                Some(super::workbench::PendingWorkbenchAction::CloseTabs {
                    ids,
                    previous_selection: state
                        .content_tabs
                        .iter()
                        .find(|tab| tab.selected)
                        .map(|tab| tab.id.clone()),
                    was_new_tab_page: state.content_browser.new_tab_page,
                });
            self.hide_desktop_content_browsers();
            cx.notify();
            return;
        }
        self.commit_close_desktop_content_tabs(ids, cx);
    }

    pub(super) fn commit_close_desktop_content_tabs(
        &mut self,
        ids: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let store =
            storage::DesktopContentTabStore::new(self.database_paths.app_db_path().to_owned());
        let expected_workspace_id = workspace_id.clone();
        let generation = self.begin_desktop_content_navigation();
        let epoch = self.desktop_workspace.content_workspace_epoch;
        let browser_locations: std::collections::HashMap<_, _> = self
            .desktop_workspace
            .content_tabs
            .iter()
            .filter(|tab| tab.kind == DesktopContentKind::Browser)
            .map(|tab| (tab.id.clone(), tab.resource.clone()))
            .collect();
        self.content_tab_controls.closing.clone_from(&ids);
        self.dismiss_content_tab_controls();
        cx.notify();
        let previous = self.desktop_workspace.content_tab_task.take();
        self.desktop_workspace.content_tab_task = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let result = cx
                .background_executor()
                .spawn(async move { store.close_batch(&workspace_id, &ids) })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.content_workspace_epoch == epoch {
                    app.content_tab_controls.closing.clear();
                }
                match result {
                    Ok(mut closed) => {
                        for id in &closed.closed_ids {
                            app.browsers.remove(content_node_id(id));
                        }
                        if app.desktop_workspace.active_id.as_deref()
                            != Some(&expected_workspace_id)
                        {
                            return;
                        }
                        let state = &mut app.desktop_workspace;
                        if state.content_tabs.iter().any(|tab| {
                            tab.kind == DesktopContentKind::ProjectFiles
                                && closed.closed_ids.contains(&tab.id)
                        }) {
                            let saved = std::mem::take(&mut state.workbench.saved);
                            let persistence = state.workbench.persistence.take();
                            state.workbench = Default::default();
                            state.workbench.saved = saved;
                            state.workbench.persistence = persistence;
                        }
                        for id in &closed.closed_ids {
                            state.content_previews.remove(id);
                            state.content_snapshots.remove(id);
                            state.content_resources.remove(id);
                            state.content_loading.remove(id);
                            state.content_load_versions.remove(id);
                            state.content_refresh.remove(id);
                        }
                        if state.content_tab_generation == generation {
                            state.content_browser.new_tab_page = false;
                        } else {
                            // Keep the committed membership and selection, but don't roll back
                            // an optimistic URL change queued after this close request.
                            for tab in &mut closed.tabs {
                                let newer = state.content_tabs.iter().find(|current| {
                                    current.id == tab.id
                                        && browser_locations
                                            .get(&current.id)
                                            .is_some_and(|url| *url != current.resource)
                                });
                                if let Some(newer) = newer {
                                    tab.resource.clone_from(&newer.resource);
                                    tab.title.clone_from(&newer.title);
                                }
                            }
                        }
                        state.content_tabs = closed.tabs;
                        app.load_selected_desktop_content(cx);
                    }
                    Err(error) => {
                        if app.desktop_workspace.active_id.as_deref()
                            == Some(&expected_workspace_id)
                        {
                            app.desktop_workspace.error = Some(ui::i18n::message(
                                "drawers.close_tabs_failed",
                                &[("error", error.to_string())],
                            ));
                        }
                    }
                }
                cx.notify();
            });
        }));
    }
}
