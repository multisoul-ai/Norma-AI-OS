//! The drawer reloads a tab by identity, even when it is not selected.
use super::super::content_node_id;
use crate::app::{browser::BrowserRefreshStatus, MultisoulApp};
use gpui::{Context, Window};
use std::collections::HashMap;
use storage::{DesktopContentKind, DesktopContentTab};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace::desktop) enum ContentRefreshStatus {
    Loading,
    Updated,
    Failed,
}

/// File and workbench feedback. Browser feedback belongs to the native load timeline.
#[derive(Default)]
pub(crate) struct ContentRefreshState(HashMap<String, ContentRefreshStatus>);

impl ContentRefreshState {
    pub(crate) fn remove(&mut self, id: &str) {
        self.0.remove(id);
    }

    pub(super) fn finish(&mut self, id: &str, success: bool) {
        if let Some(status @ ContentRefreshStatus::Loading) = self.0.get_mut(id) {
            *status = if success {
                ContentRefreshStatus::Updated
            } else {
                ContentRefreshStatus::Failed
            };
        }
    }
}

impl MultisoulApp {
    pub(super) fn refresh_desktop_content_tab(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.content_tab_can_reload(id) {
            return;
        }
        let Some(tab) = self.content_tab_by_id(id).cloned() else {
            return;
        };
        if tab.kind != DesktopContentKind::Browser {
            self.desktop_workspace
                .content_refresh
                .0
                .insert(tab.id.clone(), ContentRefreshStatus::Loading);
        }
        match tab.kind {
            DesktopContentKind::File => self.load_desktop_file_tab(tab, true, cx),
            DesktopContentKind::Browser => {
                self.browsers
                    .reload_content(content_node_id(id), &tab.resource);
                self.wake_animation_frame_loop();
            }
            DesktopContentKind::ProjectFiles => {
                if self.desktop_workspace.workbench.saved.area_id.is_none() {
                    self.ensure_file_workbench(window, cx);
                }
                if !self.desktop_workspace.workbench.refreshing {
                    self.refresh_file_workbench(cx);
                }
            }
        }
        cx.notify();
    }

    fn content_tab_by_id(&self, id: &str) -> Option<&DesktopContentTab> {
        self.desktop_workspace.content_tabs.iter().find(|tab| {
            tab.id == id && self.desktop_workspace.active_id.as_ref() == Some(&tab.workspace_id)
        })
    }

    fn explicit_refresh_status(&self, tab: &DesktopContentTab) -> Option<ContentRefreshStatus> {
        if tab.kind == DesktopContentKind::Browser {
            return match self
                .browsers
                .content_refresh_status(content_node_id(&tab.id))?
            {
                BrowserRefreshStatus::Loading => Some(ContentRefreshStatus::Loading),
                BrowserRefreshStatus::Updated => Some(ContentRefreshStatus::Updated),
                BrowserRefreshStatus::Failed => Some(ContentRefreshStatus::Failed),
            };
        }
        self.desktop_workspace
            .content_refresh
            .0
            .get(&tab.id)
            .copied()
    }

    pub(super) fn content_tab_refresh_status(&self, id: &str) -> Option<ContentRefreshStatus> {
        let tab = self.content_tab_by_id(id)?;
        let loading = match tab.kind {
            DesktopContentKind::File => self.desktop_workspace.content_loading.contains(id),
            DesktopContentKind::Browser => {
                tab.resource != "about:blank" && self.browsers.is_loading(content_node_id(id))
            }
            DesktopContentKind::ProjectFiles => {
                let state = &self.desktop_workspace.workbench;
                state.refreshing || state.loading
            }
        };
        if loading {
            return Some(ContentRefreshStatus::Loading);
        }
        self.explicit_refresh_status(tab)
    }

    pub(super) fn content_tab_is_refreshing(&self, id: &str) -> bool {
        self.content_tab_by_id(id)
            .and_then(|tab| self.explicit_refresh_status(tab))
            == Some(ContentRefreshStatus::Loading)
    }

    pub(super) fn content_tab_can_reload(&self, id: &str) -> bool {
        let Some(tab) = self.content_tab_by_id(id) else {
            return false;
        };
        !(tab.kind == DesktopContentKind::Browser && tab.resource == "about:blank")
            && self.content_tab_refresh_status(id) != Some(ContentRefreshStatus::Loading)
    }

    pub(crate) fn clear_desktop_browser_refreshes(&mut self) {
        for tab in &self.desktop_workspace.content_tabs {
            if tab.kind == DesktopContentKind::Browser {
                self.browsers
                    .clear_content_refresh(content_node_id(&tab.id));
            }
        }
    }

    pub(crate) fn desktop_workbench_is_refreshing(&self) -> bool {
        let state = &self.desktop_workspace;
        state.content_tabs.iter().any(|tab| {
            tab.kind == DesktopContentKind::ProjectFiles
                && state.content_refresh.0.get(&tab.id) == Some(&ContentRefreshStatus::Loading)
        })
    }

    pub(crate) fn cancel_desktop_workbench_refresh(&mut self) {
        let state = &mut self.desktop_workspace;
        for tab in &state.content_tabs {
            if tab.kind == DesktopContentKind::ProjectFiles
                && state.content_refresh.0.get(&tab.id) == Some(&ContentRefreshStatus::Loading)
            {
                state.content_refresh.remove(&tab.id);
            }
        }
    }

    pub(crate) fn finish_desktop_workbench_refresh(&mut self, success: bool) {
        let state = &mut self.desktop_workspace;
        for tab in &state.content_tabs {
            if tab.kind == DesktopContentKind::ProjectFiles {
                state.content_refresh.finish(&tab.id, success);
            }
        }
    }
}
