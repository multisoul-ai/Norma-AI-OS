//! 想法视图的键盘与焦点：光标在“待办、已指派”之间移动，菜单展开后改由菜单接管。

use super::super::ideas::idea_reorder::{keyboard_step, Step};
use super::super::ideas::IdeaUiTarget;
use super::super::ideas_rows::IdeasTab;
use crate::app::MultisoulApp;
use gpui::{Context, Focusable as _, Window};
use runtime::desktop_drawers::DesktopDrawerEvent;

/// 键盘光标按“待办、已指派”的显示顺序编号。
#[derive(Clone, Debug, PartialEq, Eq)]
enum Cursor {
    Todo(usize),
    Assigned(usize),
}

impl MultisoulApp {
    pub(in crate::workspace::desktop) fn left_ideas_focus(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::FocusHandle {
        self.desktop_workspace
            .left_ideas_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// 焦点在想法栏里（含输入框）时，概览自己的上下键浏览让位给想法清单。
    pub(super) fn ideas_hold_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.desktop_workspace
            .left_ideas_focus
            .as_ref()
            .is_some_and(|handle| handle.contains_focused(window, cx))
    }

    pub(super) fn select_idea_row(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let screen = self.ideas_screen(cx);
        self.desktop_workspace.ideas_cursor = screen.todo.iter().position(|row| row.id == id);
        self.left_ideas_focus(cx).focus(window);
        cx.notify();
    }

    pub(super) fn toggle_idea_menu(&mut self, id: &str, cx: &mut Context<Self>) {
        let open = self.desktop_workspace.ideas_menu.as_deref() == Some(id);
        self.desktop_workspace.ideas_menu = (!open).then(|| id.to_owned());
        self.desktop_workspace.ideas_menu_cursor = 0;
        self.desktop_workspace.ideas_notice = None;
        cx.notify();
    }

    fn cursor_at(&self, cursor: usize, todo: usize) -> Cursor {
        if cursor < todo {
            Cursor::Todo(cursor)
        } else {
            Cursor::Assigned(cursor - todo)
        }
    }

    /// 想法视图的键盘：↑/↓/Home/End 移动，回车指派（菜单里回车确认），Delete 删除，Esc 先收菜单再回对话。
    pub(super) fn handle_ideas_key(
        &mut self,
        key: &str,
        modified: bool,
        alt_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if modified && !alt_only {
            return false;
        }
        // 在输入框里打字时，方向键、回车、退格都属于输入框，不能被清单的键盘浏览抢走。
        if self
            .desktop_workspace
            .ideas_input
            .as_ref()
            .is_some_and(|input| input.focus_handle(cx).is_focused(window))
        {
            return false;
        }
        if self.desktop_workspace.ideas_aux.picker.is_some() {
            return self.handle_idea_picker_key(key, window, cx);
        }
        if key == "tab" && self.desktop_workspace.ideas_menu.is_none() {
            let next = crate::shortcuts::idea_assign::next_default_profile(
                self.desktop_workspace.ideas_aux.default_profile,
            );
            self.set_default_idea_profile(next, cx);
            self.desktop_workspace.ideas_notice = Some(ui::i18n::message(
                "drawers.default_agent_type",
                &[("next", format!("{next:?}"))],
            ));
            cx.notify();
            return true;
        }
        if alt_only {
            return matches!(key, "up" | "down") && self.move_idea_with_keyboard(key == "up", cx);
        }
        if key == "escape" && self.cancel_idea_drag(cx) {
            return true;
        }
        let screen = self.ideas_screen(cx);
        // 分段按钮一次只显示一类：光标只在当前分段的行里移动（已完成行不可操作）。
        let tab = self.desktop_workspace.ideas_tab;
        let todo = if tab == IdeasTab::Todo {
            screen.todo.len()
        } else {
            0
        };
        let visible_assigned = if tab == IdeasTab::Assigned {
            screen.assigned.len()
        } else {
            0
        };
        let total = todo + visible_assigned;
        if self.desktop_workspace.ideas_menu.is_some() {
            return self.handle_ideas_menu_key(key, window, cx);
        }
        // 刷新后条数可能变少：先把过期的光标收敛回有效范围，再做任何索引。
        let current =
            super::super::ideas_rows::clamp_cursor(self.desktop_workspace.ideas_cursor, total);
        self.desktop_workspace.ideas_cursor = current;
        let next = match key {
            "down" => Some(current.map_or(0, |index| (index + 1).min(total.saturating_sub(1)))),
            "up" => Some(current.map_or(0, |index| index.saturating_sub(1))),
            "home" => Some(0),
            "end" => total.checked_sub(1),
            _ => None,
        };
        if let Some(next) = next.filter(|_| total > 0) {
            self.desktop_workspace.ideas_cursor = Some(next);
            cx.notify();
            return true;
        }
        match (key, current.map(|index| self.cursor_at(index, todo))) {
            ("enter" | "space", Some(Cursor::Todo(index))) => {
                self.toggle_idea_menu(&screen.todo[index].id, cx);
                true
            }
            ("enter" | "space", Some(Cursor::Assigned(index))) => {
                let row = &screen.assigned[index];
                if let Some((area_id, node_id)) = row.area_id.clone().zip(row.node_id) {
                    self.locate_desktop_agent(&area_id, node_id, window, cx);
                }
                true
            }
            ("delete" | "backspace", Some(Cursor::Todo(index))) => {
                self.delete_idea(&screen.todo[index].id, cx);
                let remaining = total.saturating_sub(1);
                self.desktop_workspace.ideas_cursor =
                    remaining.checked_sub(1).map(|last| index.min(last));
                true
            }
            ("escape", _) => {
                self.toggle_desktop_drawer(DesktopDrawerEvent::EscapeOverview, cx);
                self.focus_left_drawer_view(window, cx);
                true
            }
            _ => false,
        }
    }

    /// `⌥↑/⌥↓`：把光标所在的待办上移或下移一格，光标跟着走。输入框、菜单展开时不接管。
    fn move_idea_with_keyboard(&mut self, up: bool, cx: &mut Context<Self>) -> bool {
        if self.desktop_workspace.ideas_menu.is_some() {
            return false;
        }
        let screen = self.ideas_screen(cx);
        let todo_ids: Vec<String> = screen.todo.iter().map(|row| row.id.clone()).collect();
        let Some(index) = self
            .desktop_workspace
            .ideas_cursor
            .filter(|index| *index < todo_ids.len())
        else {
            return false;
        };
        let step = if up { Step::Up } else { Step::Down };
        if let Some((before, cursor)) = keyboard_step(&todo_ids, index, step) {
            self.reorder_idea(&todo_ids[index], before.as_deref(), cx);
            self.desktop_workspace.ideas_cursor = Some(cursor);
            cx.notify();
        }
        true
    }

    fn handle_ideas_menu_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let options = self.idea_target_options(cx);
        let last = options.len();
        let cursor = self.desktop_workspace.ideas_menu_cursor;
        match key {
            "down" => self.desktop_workspace.ideas_menu_cursor = (cursor + 1).min(last),
            "up" => self.desktop_workspace.ideas_menu_cursor = cursor.saturating_sub(1),
            "home" => self.desktop_workspace.ideas_menu_cursor = 0,
            "end" => self.desktop_workspace.ideas_menu_cursor = last,
            "escape" => self.desktop_workspace.ideas_menu = None,
            "enter" | "space" => {
                let Some(id) = self.desktop_workspace.ideas_menu.clone() else {
                    return true;
                };
                let target = match options.get(cursor) {
                    Some(option) => Some(IdeaUiTarget::Existing {
                        area_id: option.area_id.clone(),
                        node_id: option.node_id,
                    }),
                    None => self
                        .desktop_workspace
                        .expanded_area_id
                        .clone()
                        .or_else(|| options.first().map(|option| option.area_id.clone()))
                        .map(|area_id| IdeaUiTarget::New {
                            area_id,
                            profile: self.desktop_workspace.ideas_aux.default_profile,
                        }),
                };
                if let Some(target) = target {
                    self.dispatch_idea(id, target, None, window, cx);
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }
}
