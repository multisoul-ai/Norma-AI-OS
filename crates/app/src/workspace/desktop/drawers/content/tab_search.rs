use super::chrome::{compact_tab_controls, COMPACT_TOOLS_HEIGHT};
use super::chrome_format::{
    content_icon, content_tab_title, reader_chrome_colors, READER_CHROME_HEIGHT,
};
use crate::app::MultisoulApp;
use gpui::Focusable;
use gpui::{
    anchored, deferred, div, point, prelude::*, px, rgba, svg, Context, Corner, Entity,
    FocusHandle, KeyDownEvent, ScrollHandle, Subscription, Window,
};
use runtime::desktop_drawers::content_tab_matches;
use storage::DesktopContentTab;
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::{
        Enter, Escape, IndentInline, Input, InputEvent, InputState, MoveDown, MoveUp, OutdentInline,
    },
    Icon, IconName, Selectable,
};
use ui::icons::with_alpha;

pub(super) struct TabSearch {
    input: Entity<InputState>,
    return_focus: FocusHandle,
    scroll: ScrollHandle,
    cursor: usize,
    _input_events: Subscription,
}

impl MultisoulApp {
    pub(super) fn render_content_tab_search_button(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let focus = window
            .use_keyed_state("desktop-content-find-focus", cx, |_, cx| {
                cx.focus_handle().tab_stop(true)
            })
            .read(cx)
            .clone();
        let key_focus = focus.clone();
        let focus_color = rgba(self.desktop_theme().focused_stroke);
        div()
            .id("desktop-content-find-trigger")
            .flex_none()
            .track_focus(&focus)
            .rounded(px(9.0))
            .border_1()
            .border_color(rgba(0x00000000))
            .focus(move |style| style.border_color(focus_color))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space" | "down") {
                    app.open_content_tab_search(key_focus.clone(), window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                self.tab_control_button(
                    "desktop-content-find",
                    IconName::Search,
                    "drawers.find_tab",
                    cx,
                )
                .tab_stop(false)
                .selected(self.content_tab_controls.search.is_some())
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.open_content_tab_search(focus.clone(), window, cx);
                })),
            )
    }

    fn open_content_tab_search(
        &mut self,
        return_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.content_tabs.is_empty() {
            return;
        }
        self.dismiss_content_tab_controls();
        let input = cx.new(|cx| {
            let placeholder =
                ui::i18n::input_placeholder("drawers.find_tab_placeholder", window, cx);
            InputState::new(window, cx).placeholder(placeholder)
        });
        let input_events = cx.subscribe(&input, |app, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                if let Some(search) = &mut app.content_tab_controls.search {
                    search.cursor = 0;
                    search.scroll.set_offset(point(px(0.0), px(0.0)));
                    cx.notify();
                }
            }
        });
        let cursor = self
            .desktop_workspace
            .content_tabs
            .iter()
            .position(|tab| tab.selected)
            .unwrap_or(0);
        let scroll = ScrollHandle::new();
        scroll.scroll_to_item(cursor);
        self.content_tab_controls.search = Some(TabSearch {
            input: input.clone(),
            return_focus,
            scroll,
            cursor,
            _input_events: input_events,
        });
        self.hide_desktop_content_browsers();
        input.focus_handle(cx).focus(window);
        cx.notify();
    }

    fn dismiss_content_tab_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = self.content_tab_controls.search.take() {
            search.return_focus.focus(window);
            cx.notify();
        }
    }

    fn matching_content_tabs(&self, cx: &gpui::App) -> Vec<&DesktopContentTab> {
        let Some(search) = &self.content_tab_controls.search else {
            return Vec::new();
        };
        let query = search.input.read(cx).value();
        self.desktop_workspace
            .content_tabs
            .iter()
            .filter(|tab| content_tab_matches(&query, &content_tab_title(tab), &tab.resource))
            .collect()
    }

    fn move_content_tab_search_cursor(&mut self, forward: bool, cx: &mut Context<Self>) {
        let len = self.matching_content_tabs(cx).len();
        if len == 0 {
            return;
        }
        let Some(search) = &mut self.content_tab_controls.search else {
            return;
        };
        let cursor = search.cursor.min(len - 1);
        search.cursor = if forward {
            (cursor + 1) % len
        } else {
            (cursor + len - 1) % len
        };
        search.scroll.scroll_to_item(search.cursor);
        cx.notify();
    }

    fn confirm_content_tab_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(search) = &self.content_tab_controls.search else {
            return;
        };
        let tabs = self.matching_content_tabs(cx);
        if let Some(tab) = tabs.get(search.cursor.min(tabs.len().saturating_sub(1))) {
            self.pick_content_tab_search(tab.id.clone(), window, cx);
        }
    }

    fn pick_content_tab_search(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_content_tab_search(window, cx);
        self.request_content_tab_reveal(&id);
        self.select_desktop_content_tab(&id, cx);
    }

    pub(super) fn render_content_tab_search(
        &self,
        drawer_width: f32,
        right: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let state = &self.desktop_workspace;
        let search = self.content_tab_controls.search.as_ref()?;
        let theme = self.desktop_theme();
        let matches = self.matching_content_tabs(cx);
        let cursor = search.cursor.min(matches.len().saturating_sub(1));
        // On a narrow drawer the picker can extend inward; the two-line results stay readable.
        let width =
            px(drawer_width.clamp(290.0, 380.0)).min(window.viewport_size().width - px(24.0));
        let panel = div()
            .id("desktop-content-search")
            .debug_selector(|| "desktop-content-search".into())
            .w(width)
            .p(px(7.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .bg(rgba(with_alpha(theme.panel_high, 0xff)))
            .rounded(px(12.0))
            .shadow_lg()
            .border_1()
            .border_color(rgba(with_alpha(theme.text_primary, 0x26)))
            .occlude()
            .on_mouse_down_out(cx.listener(|app, _, window, cx| {
                app.dismiss_content_tab_search(window, cx);
            }))
            .capture_action(cx.listener(|app, _: &Escape, window, cx| {
                app.dismiss_content_tab_search(window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|app, _: &IndentInline, window, cx| {
                app.dismiss_content_tab_search(window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|app, _: &OutdentInline, window, cx| {
                app.dismiss_content_tab_search(window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|app, _: &MoveUp, _, cx| {
                app.move_content_tab_search_cursor(false, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|app, _: &MoveDown, _, cx| {
                app.move_content_tab_search_cursor(true, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|app, _: &Enter, window, cx| {
                app.confirm_content_tab_search(window, cx);
                cx.stop_propagation();
            }))
            .child(
                div()
                    .id("desktop-content-search-input")
                    .debug_selector(|| "desktop-content-search-input".into())
                    .child(Input::new(&search.input).prefix(IconName::Search)),
            )
            .child(
                div()
                    .id("desktop-content-search-results")
                    .flex()
                    .flex_col()
                    .max_h(px(246.0).min((window.viewport_size().height - px(185.0)).max(px(48.0))))
                    .overflow_y_scroll()
                    .track_scroll(&search.scroll)
                    .when(matches.is_empty(), |list| {
                        list.child(
                            div()
                                .p(px(14.0))
                                .text_sm()
                                .text_color(rgba(theme.text_secondary))
                                .child(ui::i18n::text("drawers.no_matching_tabs")),
                        )
                    })
                    .children(matches.into_iter().enumerate().map(|(index, tab)| {
                        self.render_content_tab_search_row(
                            tab,
                            index == cursor,
                            width - px(36.0),
                            cx,
                        )
                    })),
            );
        let extra = if compact_tab_controls(drawer_width) {
            COMPACT_TOOLS_HEIGHT
        } else {
            0.0
        };
        let trailing = if state.content_browser.new_tab_page {
            12.0
        } else {
            50.0
        };
        let anchor = point(
            window.viewport_size().width - px(right + trailing),
            px(super::super::TOP + READER_CHROME_HEIGHT + extra),
        );
        Some(
            deferred(
                anchored()
                    .anchor(Corner::TopRight)
                    .position(anchor)
                    .snap_to_window()
                    .child(panel),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    fn render_content_tab_search_row(
        &self,
        tab: &DesktopContentTab,
        cursor: bool,
        width: gpui::Pixels,
        cx: &mut Context<Self>,
    ) -> Button {
        let theme = self.desktop_theme();
        let colors = reader_chrome_colors(theme);
        let title = content_tab_title(tab);
        let tooltip = tab.resource.clone();
        let project = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| tab.source_area_id.as_deref() == Some(area.id.as_str()));
        let source = project
            .and_then(|area| {
                std::path::Path::new(&tab.resource)
                    .strip_prefix(&area.root)
                    .ok()
            })
            .map(|relative| relative.to_string_lossy().into_owned())
            .unwrap_or_else(|| tooltip.clone());
        let id = tab.id.clone();
        let selected = tab.selected;
        Button::new(gpui::SharedString::from(format!(
            "desktop-content-search-row-{id}"
        )))
        .w_full()
        .h(px(48.0))
        .px(px(9.0))
        .tab_stop(false)
        .custom(
            ButtonCustomVariant::new(cx)
                .color(
                    rgba(if cursor {
                        colors.tab_selected
                    } else {
                        with_alpha(theme.panel_high, 0)
                    })
                    .into(),
                )
                .foreground(rgba(theme.text_primary).into())
                .border(
                    rgba(if cursor {
                        theme.focused_stroke
                    } else {
                        with_alpha(theme.panel_high, 0)
                    })
                    .into(),
                )
                .hover(rgba(colors.tab_hover).into())
                .active(rgba(colors.tab_pressed).into()),
        )
        .tooltip(tooltip)
        .child(
            div()
                .w(width)
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    svg()
                        .path(content_icon(tab.kind))
                        .w(px(14.0))
                        .h(px(14.0))
                        .flex_none()
                        .text_color(rgba(theme.text_secondary)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .items_start()
                        .child(
                            div()
                                .w_full()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_sm()
                                .child(title),
                        )
                        .child(
                            div()
                                .w_full()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_xs()
                                .text_color(rgba(theme.text_secondary))
                                .child(source),
                        ),
                )
                .when(selected, |row| {
                    row.child(
                        Icon::new(IconName::Check)
                            .size(px(14.0))
                            .text_color(rgba(theme.accent)),
                    )
                }),
        )
        .on_click(cx.listener(move |app, _, window, cx| {
            app.pick_content_tab_search(id.clone(), window, cx)
        }))
    }
}
