//! Right-drawer chrome matched to the assistant prototype reader.
//!
//! 参考 apple-design：层级是标签、页面上下文、再是内容；按下立即变色，标签切换不加动效。
//! 抽屉滑动仍由既有 motion 负责。内容贴边铺满，不再套一层内缩圆角卡片。

pub(super) use super::chrome_format::{
    content_context_title, content_icon, content_kind_label, is_http_resource,
    reader_chrome_colors, ReaderChromeColors, READER_CHROME_HEIGHT, READER_TOOLBAR_HEIGHT,
};

use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, svg, Context, Div, KeyDownEvent, Stateful, Window};
use storage::{DesktopContentKind, DesktopContentTab};
use ui::component::button::{Button, ButtonVariants};
use ui::icons::with_alpha;

pub(super) fn activates(event: &KeyDownEvent) -> bool {
    matches!(event.keystroke.key.as_str(), "enter" | "space")
}

pub(super) fn compact_tab_controls(width: f32) -> bool {
    width < 280.0
}

pub(super) const COMPACT_TOOLS_HEIGHT: f32 = 38.0;

impl MultisoulApp {
    pub(super) fn render_reader_chrome(
        &self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let colors = reader_chrome_colors(theme);
        let tabs = &self.desktop_workspace.content_tabs;
        let selected = tabs
            .iter()
            .find(|tab| tab.selected && !self.desktop_workspace.content_browser.new_tab_page);
        let compact = compact_tab_controls(width);
        let mut row = div()
            .h(px(READER_CHROME_HEIGHT))
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(10.0))
            .child(self.render_reader_collapse(colors, cx));
        let mut tools = div().flex().items_center().gap(px(6.0));
        if !tabs.is_empty() {
            row = row
                .child(self.render_reader_tabs(colors, width, window, cx))
                .child(self.render_new_browser_tab(colors, cx));
            tools = tools.child(self.render_content_tab_search_button(window, cx));
            if let Some(tab) = selected {
                tools = tools.child(self.render_content_refresh_button(&tab.id, cx));
            }
        }
        let chrome = div()
            .id("desktop-content-chrome")
            .debug_selector(|| "desktop-content-chrome".to_string())
            .flex()
            .flex_col()
            .flex_none()
            .border_b_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x2e)));
        if compact && !tabs.is_empty() {
            chrome
                .child(row)
                .child(tools.justify_end().px(px(10.0)).pb(px(6.0)))
        } else {
            chrome.child(row.child(tools))
        }
    }

    /// No page carries a tool row, so new-web-page access lives at the strip's end.
    fn render_new_browser_tab(&self, colors: ReaderChromeColors, cx: &mut Context<Self>) -> Button {
        let theme = self.desktop_theme();
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
                    .text_color(rgba(theme.text_primary))
                    .w(px(14.0))
                    .h(px(14.0)),
            )
            .custom(self.reader_control_variant(colors, cx))
            .tooltip(ui::i18n::text("drawers.new_webpage"))
            .on_click(cx.listener(|app, _, _, cx| {
                app.hide_desktop_content_browsers();
                app.desktop_workspace.content_browser.new_tab_page = true;
                cx.notify();
            }))
    }

    pub(super) fn render_reader_toolbar(
        &self,
        tab: &DesktopContentTab,
        width: f32,
        right: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let title = content_context_title(tab);
        let refresh_status = self.render_content_refresh_status(&tab.id, cx);
        let open_external = width >= 380.0 && is_http_resource(&tab.resource);
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
                    .flex_none()
                    .text_color(rgba(theme.text_secondary)),
            )
            .child(self.render_context_title(&title))
            .when(width >= 300.0, |row| {
                row.child(self.render_kind_badge(tab.kind, theme))
            })
            .children(refresh_status)
            .when(is_http_resource(&tab.resource) && !open_external, |row| {
                row.child(
                    self.tab_control_button(
                        "desktop-content-open-original",
                        ui::component::IconName::ExternalLink,
                        "drawers.open_original",
                        cx,
                    )
                    .on_click(
                        cx.listener(|app, _, _, cx| app.activate_reader_context_action(true, cx)),
                    ),
                )
            })
            .when(open_external, |row| {
                row.child(self.render_context_action(
                    ui::i18n::text("drawers.open_original"),
                    true,
                    theme,
                    cx,
                ))
            })
            .child(self.render_content_tab_actions(tab, width, right, window, cx))
    }

    fn render_reader_collapse(&self, colors: ReaderChromeColors, cx: &mut Context<Self>) -> Button {
        let theme = self.desktop_theme();
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
                    .text_color(rgba(theme.text_primary))
                    .w(px(16.0))
                    .h(px(16.0)),
            )
            .custom(self.reader_control_variant(colors, cx))
            .tooltip(ui::i18n::text("drawers.collapse_content"))
            .on_click(cx.listener(|app, _, _, cx| {
                app.toggle_desktop_drawer(
                    runtime::desktop_drawers::DesktopDrawerEvent::ToggleRight,
                    cx,
                );
            }))
    }

    fn render_context_title(&self, title: &str) -> gpui::AnyElement {
        let theme = self.desktop_theme();
        div()
            .id("desktop-content-context-title")
            .debug_selector(|| "desktop-content-context-title".to_string())
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .text_size(px(11.0))
            .text_color(rgba(theme.text_secondary))
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
            .text_color(rgba(theme.text_secondary))
            .cursor_pointer()
            .tab_index(0)
            .border_1()
            .border_color(rgba(0x00000000))
            .hover(move |style| {
                style
                    .bg(rgba(with_alpha(theme.edge_highlight, 0x18)))
                    .text_color(rgba(theme.text_primary))
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
