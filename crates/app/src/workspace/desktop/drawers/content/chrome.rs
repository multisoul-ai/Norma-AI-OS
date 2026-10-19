//! Right-drawer chrome matched to the assistant prototype reader.
//!
//! 参考 apple-design：层级是标签、页面上下文、再是内容；按下立即变色，标签切换不加动效。
//! 抽屉滑动仍由既有 motion 负责。内容贴边铺满，不再套一层内缩圆角卡片。

pub(super) use super::chrome_format::{
    content_context_title, content_icon, content_kind_label, is_http_resource,
    reader_chrome_colors, ReaderChromeColors, READER_CHROME_HEIGHT, READER_TOOLBAR_HEIGHT,
};
use super::{GLASS_TEXT, GLASS_TEXT_SECONDARY};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, svg, Context, Div, ElementId, KeyDownEvent, Stateful};
use storage::{DesktopContentKind, DesktopContentTab};
use ui::component::button::{Button, ButtonVariants};
use ui::icons::with_alpha;

fn activates(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space")
}

impl MultisoulApp {
    pub(super) fn render_reader_chrome(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let colors = reader_chrome_colors(theme);
        div()
            .id("desktop-content-chrome")
            .debug_selector(|| "desktop-content-chrome".to_string())
            .h(px(READER_CHROME_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(12.0))
            .border_b_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x2e)))
            .child(self.render_reader_collapse(colors, cx))
            .when(!self.desktop_workspace.content_tabs.is_empty(), |row| {
                row.child(self.render_reader_tabs(colors, cx))
            })
            .when(!self.desktop_workspace.content_tabs.is_empty(), |row| {
                row.child(self.render_new_browser_tab(colors, cx))
            })
    }

    /// No page carries a tool row, so new-web-page access lives at the strip's end.
    fn render_new_browser_tab(&self, colors: ReaderChromeColors, cx: &mut Context<Self>) -> Button {
        Button::new("desktop-content-new-browser")
            .debug_selector(|| "desktop-content-new-browser".to_string())
            .w(px(32.0))
            .h(px(32.0))
            .p(px(0.0))
            .flex_none()
            .rounded(px(9.0))
            .child(
                svg()
                    .path("icons/toolbar-plus.svg")
                    .text_color(rgba(GLASS_TEXT))
                    .w(px(14.0))
                    .h(px(14.0)),
            )
            .custom(self.reader_control_variant(colors, cx))
            .tooltip("新建网页")
            .on_click(cx.listener(|app, _, _, cx| {
                app.open_desktop_content_tab(
                    DesktopContentKind::Browser,
                    "about:blank".into(),
                    "浏览器".into(),
                    None,
                    cx,
                )
            }))
    }

    pub(super) fn render_reader_toolbar(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let tab = self
            .desktop_workspace
            .content_tabs
            .iter()
            .find(|tab| tab.selected)
            .cloned()
            .expect("toolbar requires a selected tab");
        let title = content_context_title(&tab);
        let open_external = is_http_resource(&tab.resource);
        div()
            .id("desktop-content-context")
            .debug_selector(|| "desktop-content-context".to_string())
            .h(px(READER_TOOLBAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(13.0))
            .border_b_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
            .text_size(px(11.0))
            .child(
                svg()
                    .path(content_icon(tab.kind))
                    .w(px(13.0))
                    .h(px(13.0))
                    .text_color(rgba(GLASS_TEXT_SECONDARY)),
            )
            .child(self.render_context_title(&title))
            .child(self.render_kind_badge(tab.kind, theme))
            .child(div().flex_1())
            .when(open_external, |row| {
                row.child(self.render_context_action("打开原网页", true, theme, cx))
            })
    }

    fn render_reader_collapse(&self, colors: ReaderChromeColors, cx: &mut Context<Self>) -> Button {
        Button::new("desktop-close-right-drawer")
            .debug_selector(|| "desktop-close-right-drawer".to_string())
            .w(px(32.0))
            .h(px(32.0))
            .p(px(0.0))
            .flex_none()
            .rounded(px(9.0))
            .child(
                svg()
                    .path("icons/panel-right-close.svg")
                    .text_color(rgba(GLASS_TEXT))
                    .w(px(16.0))
                    .h(px(16.0)),
            )
            .custom(self.reader_control_variant(colors, cx))
            .tooltip("收起内容浏览器")
            .on_click(cx.listener(|app, _, _, cx| {
                app.toggle_desktop_drawer(
                    runtime::desktop_drawers::DesktopDrawerEvent::ToggleRight,
                    cx,
                );
            }))
    }

    fn render_reader_tabs(
        &self,
        colors: ReaderChromeColors,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("desktop-content-tabs")
            .flex_1()
            .min_w(px(0.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .overflow_x_scroll()
            .children(
                self.desktop_workspace
                    .content_tabs
                    .iter()
                    .map(|tab| self.render_reader_tab(tab, colors, cx)),
            )
    }

    fn render_reader_tab(
        &self,
        tab: &DesktopContentTab,
        colors: ReaderChromeColors,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
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
            .max_w(px(185.0))
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
            .on_click(cx.listener(move |app, _, _, cx| {
                app.select_desktop_content_tab(&select_id, cx);
            }))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
                if activates(event) {
                    app.select_desktop_content_tab(&key_select_id, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                svg()
                    .path(content_icon(tab.kind))
                    .w(px(13.0))
                    .h(px(13.0))
                    .flex_none()
                    .text_color(rgba(if selected {
                        GLASS_TEXT
                    } else {
                        GLASS_TEXT_SECONDARY
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
                        GLASS_TEXT
                    } else {
                        GLASS_TEXT_SECONDARY
                    }))
                    .child(tab.title.clone()),
            )
            .child(self.render_tab_close(&close_id, theme, cx))
    }

    fn render_tab_close(
        &self,
        tab_id: &str,
        theme: ui::Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let close_id = tab_id.to_string();
        let key_id = tab_id.to_string();
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
            .tab_index(0)
            .border_1()
            .border_color(rgba(0x00000000))
            .hover(move |style| style.bg(rgba(with_alpha(theme.edge_highlight, 0x28))))
            .active(move |style| style.bg(rgba(with_alpha(theme.edge_highlight, 0x48))))
            .focus(move |style| style.border_color(rgba(theme.focused_stroke)))
            .on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                app.close_desktop_content_tab(&close_id, cx);
            }))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
                if activates(event) {
                    cx.stop_propagation();
                    app.close_desktop_content_tab(&key_id, cx);
                }
            }))
            .child(
                svg()
                    .path("icons/close.svg")
                    .w(px(11.0))
                    .h(px(11.0))
                    .text_color(rgba(GLASS_TEXT_SECONDARY)),
            )
    }

    fn render_context_title(&self, title: &str) -> gpui::AnyElement {
        div()
            .id("desktop-content-context-title")
            .debug_selector(|| "desktop-content-context-title".to_string())
            .flex_none()
            .max_w(px(240.0))
            .min_w(px(0.0))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .text_size(px(11.0))
            .text_color(rgba(GLASS_TEXT_SECONDARY))
            .child(title.to_string())
            .into_any_element()
    }

    fn render_kind_badge(&self, kind: DesktopContentKind, theme: ui::Theme) -> Stateful<Div> {
        div()
            .id("desktop-content-kind-badge")
            .debug_selector(|| "desktop-content-kind-badge".to_string())
            .flex_none()
            .px(px(5.0))
            .py(px(2.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x40)))
            .text_size(px(10.0))
            .text_color(rgba(theme.text_secondary))
            .child(content_kind_label(kind))
    }

    fn render_context_action(
        &self,
        label: &'static str,
        open_external: bool,
        theme: ui::Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id("desktop-content-open-original")
            .debug_selector(|| "desktop-content-open-original".to_string())
            .flex_none()
            .h(px(28.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .rounded(px(6.0))
            .text_size(px(11.0))
            .text_color(rgba(GLASS_TEXT_SECONDARY))
            .cursor_pointer()
            .tab_index(0)
            .border_1()
            .border_color(rgba(0x00000000))
            .hover(move |style| {
                style
                    .bg(rgba(with_alpha(theme.edge_highlight, 0x18)))
                    .text_color(rgba(GLASS_TEXT))
            })
            .active(move |style| style.bg(rgba(with_alpha(theme.accent_blue, 0x30))))
            .focus(move |style| style.border_color(rgba(theme.focused_stroke)))
            .on_click(cx.listener(move |app, _, _, cx| {
                app.activate_reader_context_action(open_external, cx);
            }))
            .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
                if activates(event) {
                    app.activate_reader_context_action(open_external, cx);
                    cx.stop_propagation();
                }
            }))
            .child(label)
    }

    fn activate_reader_context_action(&mut self, open_external: bool, _cx: &mut Context<Self>) {
        if !open_external {
            return;
        }
        let url = self
            .desktop_workspace
            .content_tabs
            .iter()
            .find(|tab| tab.selected && is_http_resource(&tab.resource))
            .map(|tab| tab.resource.clone());
        if let Some(url) = url {
            self.open_url_in_default_browser(url);
        }
    }
}
