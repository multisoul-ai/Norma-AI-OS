use super::*;
use gpui::{EntityInputHandler, Focusable, KeyDownEvent, ScrollHandle};

#[derive(Default)]
pub(crate) struct HomeState {
    pub search_open: bool,
    // Closing keeps the capsule focused; its deferred Focus must not reopen it.
    // Explicit opening, typing, or leaving the input ends this dismissed focus.
    pub search_focus_dismissed: bool,
    pub cursor: usize,
    pub menu_cursor: usize,
    pub older_expanded: bool,
    pub search_scroll: Option<Entity<ScrollHandle>>,
    pub search_input: Option<Entity<InputState>>,
    pub panel_focus: Option<gpui::FocusHandle>,
}

pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as i64)
}

impl MultisoulApp {
    pub(super) fn cancel_home_ime(
        &mut self,
        _: &ui::component::input::Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.desktop_workspace.home.search_input.clone() else {
            return;
        };
        if !input.focus_handle(cx).is_focused(window) {
            return;
        }
        // Input's Escape action unmarks then propagates before raw key capture.
        // Consume this action while composing so the same Esc cannot close search.
        let cancelled = input.update(cx, |input, cx| {
            if input.marked_text_range(window, cx).is_none() {
                return false;
            }
            input.unmark_text(window, cx);
            cx.notify();
            true
        });
        if cancelled {
            cx.stop_propagation();
        }
    }

    pub(crate) fn capture_home_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.screen != crate::app::AppScreen::Launchpad
            || !self.desktop_workspace.home_directory
            || self.desktop_workspace.workspaces.is_empty()
            || self.feature_guide.is_visible()
            || self.desktop_directory_zoom_active()
        {
            return;
        }
        if let Some(input) = self.desktop_workspace.home.search_input.clone() {
            self.capture_home_search_key(&input, event, window, cx);
        }
    }

    pub(super) fn open_home_search(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.desktop_workspace.home.search_open = true;
        self.desktop_workspace.home.search_focus_dismissed = false;
        self.desktop_workspace.actions_workspace_id = None;
        input.focus_handle(cx).focus(window);
        cx.notify();
    }

    pub(super) fn close_home_search(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.desktop_workspace.search_query.clear();
        input.update(cx, |input, cx| input.set_value("", window, cx));
        // GPUI emits Focus after drawing, including focus restored from a panel button.
        self.desktop_workspace.home.search_focus_dismissed = true;
        input.focus_handle(cx).focus(window);
        self.desktop_workspace.home.search_open = false;
        self.desktop_workspace.home.cursor = 0;
        self.desktop_workspace.home.older_expanded = false;
        if let Some(scroll) = &self.desktop_workspace.home.search_scroll {
            scroll.read(cx).set_offset(gpui::point(px(0.0), px(0.0)));
        }
        cx.notify();
    }

    pub(super) fn home_search_groups(&self, cx: &gpui::App) -> Vec<catalog::HomeGroup> {
        catalog::groups(
            &self.home_entries(cx),
            &self.desktop_workspace.search_query,
            self.desktop_workspace.home.older_expanded,
            now_ms(),
        )
    }

    pub(super) fn create_home_search_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.desktop_workspace.busy {
            return;
        }
        let query = self.desktop_workspace.search_query.trim().to_string();
        self.desktop_workspace.home.search_open = false;
        self.create_desktop_workspace(window, cx);
        if let Some(input) = self.desktop_workspace.create_input.clone() {
            input.update(cx, |input, cx| input.set_value(query, window, cx));
        }
    }

    pub(super) fn capture_home_search_key(
        &mut self,
        input: &Entity<InputState>,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_navigation_blocked(cx)
            || self.command_input_focused(window, cx)
            || self.desktop_workspace.actions_workspace_id.is_some()
        {
            return;
        }
        let key = &event.keystroke;
        if input.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        }) {
            return;
        }
        if key.modifiers.platform || key.modifiers.control || key.modifiers.alt {
            return;
        }
        let open = self.desktop_workspace.home.search_open;
        let input_focused = input.focus_handle(cx).is_focused(window);
        if open
            && (key.key == "escape"
                || (input_focused && matches!(key.key.as_str(), "up" | "down" | "enter")))
        {
            let groups = self.home_search_groups(cx);
            let entries: Vec<_> = groups.iter().flat_map(|group| &group.entries).collect();
            match key.key.as_str() {
                "escape" => self.close_home_search(input, window, cx),
                "enter" => {
                    if self.desktop_workspace.busy {
                        return;
                    }
                    if let Some(entry) = entries.get(
                        self.desktop_workspace
                            .home
                            .cursor
                            .min(entries.len().saturating_sub(1)),
                    ) {
                        let id = entry.id.clone();
                        self.close_home_search(input, window, cx);
                        self.open_desktop_workspace(id, window, cx);
                    } else if !self.desktop_workspace.search_query.trim().is_empty() {
                        self.create_home_search_query(window, cx);
                    }
                }
                _ => {
                    let cursor = &mut self.desktop_workspace.home.cursor;
                    *cursor = if key.key == "down" {
                        (*cursor + 1).min(entries.len().saturating_sub(1))
                    } else {
                        cursor.saturating_sub(1)
                    };
                    let mut offset = 0;
                    let mut remaining = *cursor;
                    for group in &groups {
                        offset += 1;
                        if remaining < group.entries.len() {
                            offset += remaining;
                            break;
                        }
                        remaining -= group.entries.len();
                        offset += group.entries.len();
                    }
                    if let Some(scroll) = &self.desktop_workspace.home.search_scroll {
                        scroll.read(cx).scroll_to_item(offset);
                    }
                    cx.notify();
                }
            }
            cx.stop_propagation();
        } else if !open
            && input.focus_handle(cx).is_focused(window)
            && matches!(key.key.as_str(), "up" | "down" | "enter")
        {
            self.open_home_search(input, window, cx);
            cx.stop_propagation();
        } else if !open && !input_focused {
            if let Some(text) = key
                .key_char
                .as_deref()
                .filter(|text| !text.trim().is_empty() && !text.chars().any(char::is_control))
            {
                self.open_home_search(input, window, cx);
                input.update(cx, |input, cx| {
                    input.set_value(text.to_string(), window, cx)
                });
                self.desktop_workspace.search_query = text.to_string();
                self.desktop_workspace.home.cursor = 0;
                cx.stop_propagation();
            }
        }
    }
}
