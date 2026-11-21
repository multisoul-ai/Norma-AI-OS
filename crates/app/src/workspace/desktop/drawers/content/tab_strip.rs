//! Scrollable reader labels and their direct input handlers.
use super::chrome::{activates, compact_tab_controls};
use super::chrome_format::{
    content_icon, content_tab_title, ReaderChromeColors, READER_CHROME_HEIGHT,
};
use crate::app::MultisoulApp;
use gpui::{
    div, point, prelude::*, px, rgba, svg, Context, Div, ElementId, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseUpEvent, Stateful, Window,
};
use storage::DesktopContentTab;
use ui::icons::with_alpha;

impl MultisoulApp {
    pub(super) fn render_reader_tabs(
        &self,
        colors: ReaderChromeColors,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let strip = &self.content_tab_controls.strip;
        let scroll = strip.clone();
        let reserved = if compact_tab_controls(width) {
            96.0
        } else {
            172.0
        };
        let max_tab_width = (width - reserved).clamp(76.0, 185.0);
        div()
            .id("desktop-content-tabs")
            .flex_1()
            .min_w(px(0.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .overflow_x_scroll()
            .track_scroll(strip)
            .on_scroll_wheel(move |event, window, cx| {
                let mut offset = scroll.offset();
                let delta = event.delta.pixel_delta(px(20.0));
                let max = scroll.max_offset().width;
                if !event.modifiers.control && max > px(0.0) && delta.y.abs() > delta.x.abs() {
                    offset.x = (offset.x + delta.y).clamp(-max, px(0.0));
                    scroll.set_offset(offset);
                    window.refresh();
                    cx.stop_propagation();
                }
            })
            .children(
                self.desktop_workspace
                    .content_tabs
                    .iter()
                    .enumerate()
                    .map(|(index, tab)| {
                        self.render_reader_tab(tab, index, colors, max_tab_width, window, cx)
                    }),
            )
    }

    fn render_reader_tab(
        &self,
        tab: &DesktopContentTab,
        index: usize,
        colors: ReaderChromeColors,
        max_width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let focus = window
            .use_keyed_state(
                gpui::SharedString::from(format!("desktop-tab-focus-{}", tab.id)),
                cx,
                |_, cx| cx.focus_handle().tab_stop(true),
            )
            .read(cx)
            .clone();
        let group = format!("desktop-tab-hover-{}", tab.id);
        let focus_visible = focus.is_focused(window);
        let menu_focus = focus.clone();
        let select_focus = focus.clone();
        let mouse_focus = focus.clone();
        let mouse_menu_id = tab.id.clone();
        let middle_down_id = tab.id.clone();
        let middle_id = tab.id.clone();
        let middle_out_id = tab.id.clone();
        let strip = self.content_tab_controls.strip.clone();
        let selected = tab.selected;
        let select_id = tab.id.clone();
        let key_select_id = select_id.clone();
        let close_id = tab.id.clone();
        let fill = if selected {
            colors.tab_selected
        } else {
            colors.tab_rest
        };
        let hover = if selected {
            colors.tab_selected_hover
        } else {
            colors.tab_hover
        };
        let pressed = if selected {
            colors.tab_selected_pressed
        } else {
            colors.tab_pressed
        };
        let border = if selected {
            colors.tab_selected_border
        } else {
            colors.tab_rest_border
        };
        div()
            .id(ElementId::Name(
                format!("desktop-content-tab-{}", tab.id).into(),
            ))
            .debug_selector({
                let id = tab.id.clone();
                move || format!("desktop-content-tab-{id}")
            })
            .h(px(32.0))
            .max_w(px(max_width))
            .group(group.clone())
            .track_focus(&focus)
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.0))
            .pl(px(8.0))
            .pr(px(4.0))
            .rounded(px(9.0))
            .border_1()
            .border_color(rgba(border))
            .bg(rgba(fill))
            .cursor_pointer()
            .tab_index(0)
            .hover(move |style| style.bg(rgba(hover)))
            .active(move |style| style.bg(rgba(pressed)))
            .focus(move |style| style.border_color(rgba(theme.focused_stroke)))
            .on_click(cx.listener(move |app, _, window, cx| {
                select_focus.focus(window);
                app.request_content_tab_reveal(&select_id);
                app.select_desktop_content_tab(&select_id, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                    app.open_content_tab_menu(
                        &mouse_menu_id,
                        event.position,
                        mouse_focus.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |app, _, _, cx| {
                    app.content_tab_controls.middle_pressed = Some(middle_down_id.clone());
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Middle,
                cx.listener(move |app, _, _, _| {
                    if app.content_tab_controls.middle_pressed.as_ref() == Some(&middle_out_id) {
                        app.content_tab_controls.middle_pressed = None;
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |app, _: &MouseUpEvent, _, cx| {
                    if app.content_tab_controls.middle_pressed.take().as_ref() == Some(&middle_id) {
                        app.close_desktop_content_tab(&middle_id, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "f10" && event.keystroke.modifiers.shift {
                    let anchor = strip
                        .bounds_for_item(index)
                        .map(|bounds| bounds.bottom_right() + strip.offset())
                        .unwrap_or_else(|| {
                            point(
                                window.viewport_size().width - px(12.0),
                                px(READER_CHROME_HEIGHT),
                            )
                        });
                    app.open_content_tab_menu(
                        &key_select_id,
                        anchor,
                        menu_focus.clone(),
                        window,
                        cx,
                    );
                } else if activates(event) {
                    app.request_content_tab_reveal(&key_select_id);
                    app.select_desktop_content_tab(&key_select_id, cx);
                } else {
                    let tabs = &app.desktop_workspace.content_tabs;
                    let Some(current) = tabs.iter().position(|tab| tab.id == key_select_id) else {
                        return;
                    };
                    let target = match event.keystroke.key.as_str() {
                        "left" => (current + tabs.len() - 1) % tabs.len(),
                        "right" => (current + 1) % tabs.len(),
                        "home" => 0,
                        "end" => tabs.len() - 1,
                        _ => return,
                    };
                    let target = tabs[target].id.clone();
                    window
                        .use_keyed_state(
                            gpui::SharedString::from(format!("desktop-tab-focus-{target}")),
                            cx,
                            |_, cx| cx.focus_handle().tab_stop(true),
                        )
                        .read(cx)
                        .clone()
                        .focus(window);
                    app.request_content_tab_reveal(&target);
                    app.select_desktop_content_tab(&target, cx);
                }
                cx.stop_propagation();
            }))
            .child(
                svg()
                    .path(content_icon(tab.kind))
                    .w(px(13.0))
                    .h(px(13.0))
                    .flex_none()
                    .text_color(rgba(if selected {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .max_w(px(112.0))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(11.0))
                    .text_color(rgba(if selected {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    }))
                    .child(content_tab_title(tab)),
            )
            .child(self.render_tab_close(&close_id, group, focus_visible, theme, cx))
    }

    fn render_tab_close(
        &self,
        tab_id: &str,
        group: String,
        focused: bool,
        theme: ui::Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let close_id = tab_id.to_string();
        div()
            .id(ElementId::Name(
                format!("desktop-content-close-{tab_id}").into(),
            ))
            .w(px(22.0))
            .h(px(22.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .border_1()
            .border_color(rgba(0x00000000))
            .opacity(if focused { 1.0 } else { 0.0 })
            .group_hover(group, |style| style.opacity(1.0))
            .hover(move |style| style.bg(rgba(with_alpha(theme.edge_highlight, 0x28))))
            .active(move |style| style.bg(rgba(with_alpha(theme.edge_highlight, 0x48))))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.close_desktop_content_tab(&close_id, cx);
            }))
            .child(
                svg()
                    .path("icons/close.svg")
                    .w(px(11.0))
                    .h(px(11.0))
                    .text_color(rgba(theme.text_secondary)),
            )
    }
}
