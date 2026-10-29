use crate::app::{AppScreen, MultisoulApp};
use crate::shortcuts::ShortcutCommandId;
use canvas::NavigationDirection;
use gpui::{point, px, Context, EntityInputHandler, FocusHandle, Focusable, KeyDownEvent, Window};

#[derive(Default)]
pub(crate) struct DirectorySelection {
    pub selected: Option<String>,
    pub search_focus: Option<FocusHandle>,
}

impl MultisoulApp {
    pub(super) fn capture_directory_input_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Input 的动作先于原始按键路由消费导航键；空输入时走同一目录路由。
        self.capture_directory_key(
            &KeyDownEvent {
                keystroke: gpui::Keystroke::parse(key).expect("固定目录输入键位合法"),
                is_held: false,
            },
            window,
            cx,
        );
    }

    fn directory_navigation_ready(&self, window: &Window, cx: &Context<Self>) -> bool {
        let state = &self.desktop_workspace;
        self.screen == AppScreen::Launchpad
            && !state.home_directory
            && !state.busy
            && state.transition.is_none()
            && state.pending_navigation.is_none()
            && state.actions_workspace_id.is_none()
            && !self.feature_guide.is_visible()
            && !self.desktop_navigation_blocked(cx)
            && !self.command_input_focused(window, cx)
            && !state
                .directory
                .search_focus
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window))
            && !state.switcher_input.as_ref().is_some_and(|input| {
                input.focus_handle(cx).is_focused(window) && !input.read(cx).value().is_empty()
            })
    }

    /// 切换器空搜索框也可直接浏览；非空搜索与输入法组合仍交给输入控件。
    pub(crate) fn capture_directory_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.directory_navigation_ready(window, cx) {
            return;
        }
        if let Some(input) = self.desktop_workspace.switcher_input.clone() {
            if input.focus_handle(cx).is_focused(window)
                && input.update(cx, |input, cx| {
                    input.marked_text_range(window, cx).is_some()
                })
            {
                return;
            }
        }
        if let Some(command) = self.shortcuts.command_for_keystroke(&event.keystroke) {
            if command == ShortcutCommandId::ExpandSelectedProject {
                self.enter_selected_desktop_workspace(window, cx);
                cx.stop_propagation();
            }
            // 用户配置的命令优先于网格裸方向键。
            return;
        }
        if event.keystroke.modifiers.modified() {
            return;
        }
        let direction = match event.keystroke.key.as_str() {
            "left" => NavigationDirection::Left,
            "right" => NavigationDirection::Right,
            "up" => NavigationDirection::Up,
            "down" => NavigationDirection::Down,
            _ => return,
        };
        let items = self.desktop_workspace.filtered();
        let current = items
            .iter()
            .position(|item| self.desktop_workspace.directory.selected.as_ref() == Some(&item.id));
        let size = window.viewport_size();
        let layout =
            super::layout::workspace_layout(size.width.into(), size.height.into(), items.len());
        let columns = layout
            .iter()
            .take_while(|rect| rect.origin.y == layout[0].origin.y)
            .count();
        if let Some(index) = runtime::desktop_navigation::directory_grid_target(
            items.len(),
            columns,
            current,
            direction,
        ) {
            self.desktop_workspace.directory.selected = Some(items[index].id.clone());
            let rect = layout[index];
            let scroll = &self.desktop_directory_scroll;
            let height = f32::from(scroll.bounds().size.height);
            let mut offset = f32::from(scroll.offset().y);
            // 62px切换器顶栏覆盖在滚动层上方，目标不能滚到它的背后。
            if rect.origin.y + offset < 80.0 {
                offset = 80.0 - rect.origin.y;
            } else if rect.max().y + offset > height - 8.0 {
                offset = height - 8.0 - rect.max().y;
            }
            scroll.set_offset(point(px(0.0), px(offset.min(0.0))));
            cx.notify();
        }
        cx.stop_propagation();
    }

    pub(super) fn enter_selected_desktop_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.directory_navigation_ready(window, cx) {
            return;
        }
        let items = self.desktop_workspace.filtered();
        let selected = self
            .desktop_workspace
            .directory
            .selected
            .as_ref()
            .or(self.desktop_workspace.active_id.as_ref());
        let target = items
            .iter()
            .find(|item| Some(&item.id) == selected)
            .or_else(|| items.first())
            .map(|item| item.id.clone());
        if let Some(id) = target {
            self.desktop_workspace.directory.selected = Some(id.clone());
            self.open_desktop_workspace(id, window, cx);
        }
    }
}
