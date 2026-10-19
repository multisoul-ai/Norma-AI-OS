//! Current visible desktop location for each Norma model turn.

use crate::app::{AppScreen, MultisoulApp};
use agent::DesktopContext;
use storage::{DesktopProjectArea, DesktopWorkspace};

pub(super) fn resolve_desktop_context(
    screen: AppScreen,
    workspace: Option<&DesktopWorkspace>,
    areas: &[DesktopProjectArea],
    expanded_area_id: Option<&str>,
    visible_session_ready: bool,
) -> Option<DesktopContext> {
    match screen {
        AppScreen::Canvas => None,
        AppScreen::Launchpad => Some(DesktopContext::NoWorkspace),
        AppScreen::DesktopWorkspace => {
            let Some(workspace) = workspace else {
                return Some(DesktopContext::NoWorkspace);
            };
            let Some(area_id) = expanded_area_id else {
                return Some(DesktopContext::WorkspaceOverview {
                    workspace_id: workspace.id.clone(),
                    workspace_name: workspace.name.clone(),
                });
            };
            if visible_session_ready {
                if let Some(area) = areas.iter().find(|area| area.id == area_id) {
                    return Some(DesktopContext::ProjectCanvas {
                        workspace_id: workspace.id.clone(),
                        workspace_name: workspace.name.clone(),
                        area_id: area.id.clone(),
                        project_name: area.display_name.clone(),
                    });
                }
            }
            Some(DesktopContext::Switching {
                workspace_id: workspace.id.clone(),
                workspace_name: workspace.name.clone(),
            })
        }
    }
}

impl MultisoulApp {
    pub(crate) fn desktop_context_for_provider(&self, cx: &gpui::App) -> Option<DesktopContext> {
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            return host.read(cx).desktop_context_on_host();
        }
        self.desktop_context_on_host()
    }

    fn desktop_context_on_host(&self) -> Option<DesktopContext> {
        let workspace = self.desktop_workspace.active();
        let expanded = self.desktop_workspace.expanded_area_id.as_deref();
        let visible_session_ready = workspace.zip(expanded).is_some_and(|(workspace, area_id)| {
            self.desktop_workspace
                .sessions
                .contains_key(&(workspace.id.clone(), area_id.to_owned()))
        });
        resolve_desktop_context(
            self.screen,
            workspace,
            &self.desktop_workspace.areas,
            expanded,
            visible_session_ready,
        )
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_context_tests.rs"]
mod tests;
