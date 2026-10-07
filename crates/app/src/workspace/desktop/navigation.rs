use super::scene::DesktopNavigation;
use crate::app::{AppScreen, MultisoulApp};
use crate::shortcuts::ShortcutCommandId;
use gpui::{Context, Window};
use runtime::desktop_navigation::adjacent_target;

impl MultisoulApp {
    pub(crate) fn navigate_desktop_shortcut(
        &mut self,
        command: ShortcutCommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_navigation_blocked(cx) {
            return;
        }
        if let Some(host) = self.desktop_host.as_ref().and_then(|host| host.upgrade()) {
            // 先结束子项目当前的可变借用；导航可能马上分离或重新聚焦同一个项目。
            window.defer(cx, move |window, cx| {
                host.update(cx, |host, cx| {
                    host.navigate_desktop_shortcut(command, window, cx)
                });
            });
            return;
        }
        let state = &self.desktop_workspace;
        match command {
            ShortcutCommandId::NewWorkspace => self.create_desktop_workspace(window, cx),
            ShortcutCommandId::AddProject
                if matches!(self.screen, AppScreen::DesktopWorkspace)
                    && state.active_id.is_some() =>
            {
                self.open_desktop_add_project(window, cx);
            }
            ShortcutCommandId::SwitchWorkspace => self.open_desktop_switcher(window, cx),
            ShortcutCommandId::PreviousWorkspace | ShortcutCommandId::NextWorkspace => {
                let current = match state.pending_navigation.as_ref() {
                    Some(DesktopNavigation::Workspace(id)) => Some(id.as_str()),
                    _ => state.active_id.as_deref(),
                };
                let target = adjacent_target(
                    state
                        .workspaces
                        .iter()
                        .map(|item| (item.created_at_unix_ms, item.id.as_str())),
                    current,
                    command == ShortcutCommandId::PreviousWorkspace,
                )
                .map(str::to_owned);
                if let Some(id) = target {
                    if state.active_id.as_ref() != Some(&id)
                        || state.pending_navigation.is_some()
                        || matches!(self.screen, AppScreen::Launchpad)
                    {
                        self.open_desktop_workspace(id, window, cx);
                        window.focus(&self.focus_handle);
                    }
                }
            }
            ShortcutCommandId::PreviousProject | ShortcutCommandId::NextProject
                if matches!(self.screen, AppScreen::DesktopWorkspace)
                    && !state.busy
                    && !matches!(
                        state.pending_navigation,
                        Some(DesktopNavigation::Workspace(_))
                    ) =>
            {
                if let Some(id) = self.desktop_overview_selection() {
                    self.close_desktop_switcher(cx);
                    self.expand_desktop_project_area(id, window, cx);
                    return;
                }
                let current = match state.pending_navigation.as_ref() {
                    Some(DesktopNavigation::Project(id)) => Some(id.as_str()),
                    _ => state
                        .expanded_area_id
                        .as_deref()
                        .or(state.active_area_id.as_deref()),
                };
                let target = adjacent_target(
                    state
                        .areas
                        .iter()
                        .map(|item| (item.added_at_unix_ms, item.id.as_str())),
                    current,
                    command == ShortcutCommandId::PreviousProject,
                )
                .map(str::to_owned);
                if let Some(id) = target {
                    self.close_desktop_switcher(cx);
                    self.expand_desktop_project_area(id, window, cx);
                }
            }
            ShortcutCommandId::ShowProjectOverview
                if matches!(self.screen, AppScreen::DesktopWorkspace) =>
            {
                self.close_desktop_switcher(cx);
                self.collapse_desktop_project_area(window, cx);
            }
            ShortcutCommandId::ShowWorkspaceDirectory => {
                self.close_desktop_switcher(cx);
                self.show_desktop_launchpad(window, cx);
                window.focus(&self.focus_handle);
            }
            _ => {}
        }
    }

    /// 表单、Settings 与确认弹窗期间，桌面导航和总览选择都保持静止。
    fn desktop_navigation_blocked(&self, cx: &Context<Self>) -> bool {
        let state = &self.desktop_workspace;
        self.settings_panel.open
            || self
                .expanded_desktop_project()
                .is_some_and(|project| project.read(cx).settings_panel.open)
            || self.clone_project.is_open()
            || state.create_input.is_some()
            || state.add_project_open
            || state.rename_target.is_some()
            || state.pending_delete.is_some()
            || state.pending_project_path.is_some()
    }

    fn desktop_overview_active(&self) -> bool {
        let state = &self.desktop_workspace;
        matches!(self.screen, AppScreen::DesktopWorkspace)
            && state.expanded_area_id.is_none()
            && state.transition.is_none()
            && state.pending_navigation.is_none()
            && !state.busy
            && !state.switcher_open
    }

    /// 项目总览中仍存在的键盘选中项目；展开项目内或选中项已被删除时为 None。
    pub(crate) fn desktop_overview_selection(&self) -> Option<String> {
        let state = &self.desktop_workspace;
        if !self.desktop_overview_active() {
            return None;
        }
        let selected = state.selected_area_id.as_deref()?;
        state
            .areas
            .iter()
            .any(|area| area.id == selected)
            .then(|| selected.to_owned())
    }

    /// 总览中裸 ←/→ 在项目间循环移动选中态；返回是否已消费该按键。
    pub(crate) fn move_desktop_overview_selection(
        &mut self,
        previous: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.desktop_overview_active()
            || self.desktop_navigation_blocked(cx)
            || self.desktop_workspace.areas.is_empty()
        {
            return false;
        }
        let current = self.desktop_overview_selection();
        let target = adjacent_target(
            self.desktop_workspace
                .areas
                .iter()
                .map(|item| (item.added_at_unix_ms, item.id.as_str())),
            current.as_deref(),
            previous,
        )
        .map(str::to_owned);
        self.desktop_workspace.selected_area_id = target;
        cx.notify();
        true
    }

    /// Settings 在任一项目中修改后，同一窗口的宿主和后台项目使用相同键位。
    pub(crate) fn sync_desktop_shortcuts(&self, cx: &mut Context<Self>) {
        let prefs = self.shortcuts.prefs.clone();
        let effective = self.shortcuts.effective.clone();
        let host = self
            .desktop_host
            .as_ref()
            .and_then(|host| host.upgrade())
            .unwrap_or_else(|| cx.entity());
        cx.defer(move |cx| {
            let sessions = host.update(cx, |host, cx| {
                host.shortcuts.prefs = prefs.clone();
                host.shortcuts.effective = effective.clone();
                cx.notify();
                host.desktop_workspace
                    .sessions
                    .values()
                    .cloned()
                    .collect::<Vec<_>>()
            });
            for session in sessions {
                session.update(cx, |session, cx| {
                    session.shortcuts.prefs = prefs.clone();
                    session.shortcuts.effective = effective.clone();
                    cx.notify();
                });
            }
        });
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_navigation_shortcuts_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_overview_selection_tests.rs"]
mod overview_selection_tests;
