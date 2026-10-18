//! The right content drawer and its workspace-scoped tabs.

use super::{
    glass_button_variant, is_image_file, is_web_file, BOTTOM, GLASS_TEXT, GLASS_TEXT_SECONDARY, TOP,
};
use crate::app::MultisoulApp;
use gpui::{
    div, point, prelude::*, px, rgba, svg, BoxShadow, Context, Div, ElementId, MouseButton,
    Stateful, Window,
};
use runtime::desktop_drawers::DesktopDrawerEvent;
use storage::DesktopContentKind;
use ui::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    StyledExt,
};
use ui::icons::with_alpha;

mod preview_load;
mod render;
mod tabs;

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_content_tab_order_tests.rs"]
mod tests;

impl MultisoulApp {
    fn open_desktop_url_from_input(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.desktop_workspace.content_address_input.as_ref() else {
            return;
        };
        let draft = input.read(cx).value().trim().to_string();
        let url = crate::cards::browser::normalize_browser_url(&draft);
        let title = url::Url::parse(&url)
            .ok()
            .and_then(|url| url.host_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "浏览器".to_string());
        self.open_desktop_content_tab(
            DesktopContentKind::Browser,
            url,
            title,
            self.desktop_workspace.expanded_area_id.clone(),
            cx,
        );
    }

    pub(super) fn render_assistant_right(
        &mut self,
        width: f32,
        edge_inset: f32,
        opacity: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if self.desktop_workspace.content_address_input.is_none() {
            self.desktop_workspace.content_address_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("输入网页地址")));
        }
        let theme = self.desktop_theme();
        let frost = self.render_drawer_frost(false, width, edge_inset, window);
        let selected = self
            .desktop_workspace
            .content_tabs
            .iter()
            .find(|tab| tab.selected)
            .cloned();
        let show_address_bar = selected
            .as_ref()
            .is_some_and(|tab| tab.kind == DesktopContentKind::Browser);
        let height = (f32::from(window.viewport_size().height)
            - TOP
            - BOTTOM
            - if show_address_bar { 158.0 } else { 108.0 })
        .max(100.0);
        div()
            .id("desktop-assistant-right")
            .debug_selector(|| "desktop-assistant-right".to_string())
            .absolute()
            .right(px(edge_inset))
            .top(px(TOP))
            .bottom(px(BOTTOM))
            .w(px(width))
            .opacity(opacity)
            .flex()
            .flex_col()
            .rounded(px(0.0))
            .overflow_hidden()
            .border_l_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x55)))
            .shadow(vec![BoxShadow {
                color: rgba(0x00000070).into(),
                offset: point(px(-14.0), px(0.0)),
                blur_radius: px(48.0),
                spread_radius: px(0.0),
            }])
            .text_color(rgba(GLASS_TEXT))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(frost)
            .child(
                div()
                    .h(px(52.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(12.0))
                    .border_b_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x36)))
                    .bg(rgba(with_alpha(theme.canvas_from, 0x7a)))
                    .child(
                        Button::new("desktop-close-right-drawer")
                            .debug_selector(|| "desktop-close-right-drawer".to_string())
                            .w(px(32.0))
                            .h(px(32.0))
                            .p(px(0.0))
                            .child(
                                svg()
                                    .path("icons/panel-right-close.svg")
                                    .text_color(rgba(GLASS_TEXT))
                                    .w(px(17.0))
                                    .h(px(17.0)),
                            )
                            .custom(glass_button_variant(theme, cx))
                            .tooltip("收起内容浏览器")
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.toggle_desktop_drawer(DesktopDrawerEvent::ToggleRight, cx);
                            })),
                    )
                    .when(!self.desktop_workspace.content_tabs.is_empty(), |row| {
                        row.child(self.render_content_tabs(cx))
                    }),
            )
            .when(show_address_bar, |drawer| {
                drawer.child(self.render_content_address_bar(cx))
            })
            .child(
                div()
                    .mx(px(12.0))
                    .mb(px(12.0))
                    .flex_1()
                    .min_h(px(0.0))
                    .rounded(px(12.0))
                    .bg(rgba(with_alpha(
                        theme.graphite,
                        if selected.is_some() { 0x48 } else { 0x00 },
                    )))
                    .when_some(selected, |body, tab| {
                        body.child(self.render_content_tab_body(&tab, width, height, cx))
                    })
                    .when(self.desktop_workspace.content_tabs.is_empty(), |body| {
                        body.child(
                            div()
                                .id("desktop-content-empty")
                                .debug_selector(|| "desktop-content-empty".to_string())
                                .size_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(9.0))
                                .text_color(rgba(GLASS_TEXT_SECONDARY))
                                .child(
                                    svg()
                                        .path("icons/file-document.svg")
                                        .w(px(28.0))
                                        .h(px(28.0))
                                        .text_color(rgba(GLASS_TEXT)),
                                )
                                .child(
                                    div()
                                        .font_semibold()
                                        .text_size(px(18.0))
                                        .text_color(rgba(GLASS_TEXT))
                                        .child("在这里浏览内容"),
                                )
                                .child(
                                    div()
                                        .max_w(px(250.0))
                                        .text_center()
                                        .text_size(px(11.0))
                                        .child("与 Norma 对话即可打开进度、网页、文件或媒体；你可以同时保留多个标签。"),
                                ),
                        )
                    }),
            )
    }

    fn render_content_address_bar(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let input = self
            .desktop_workspace
            .content_address_input
            .as_ref()
            .expect("content input initialized");
        div()
            .id("desktop-content-address-bar")
            .debug_selector(|| "desktop-content-address-bar".to_string())
            .h(px(42.0))
            .mx(px(12.0))
            .mb(px(8.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(34.0))
                    .px(px(10.0))
                    .rounded(px(9.0))
                    .bg(rgba(with_alpha(theme.graphite, 0xa0)))
                    .child(Input::new(input).appearance(false)),
            )
            .child(
                Button::new("desktop-content-open-url")
                    .debug_selector(|| "desktop-content-open-url".to_string())
                    .label("打开")
                    .custom(glass_button_variant(theme, cx))
                    .on_click(cx.listener(|app, _, _, cx| app.open_desktop_url_from_input(cx))),
            )
    }

    fn render_content_tabs(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.desktop_theme();
        div()
            .id("desktop-content-tabs")
            .flex_1()
            .min_w(px(0.0))
            .h(px(36.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .px(px(2.0))
            .overflow_x_scroll()
            .children(self.desktop_workspace.content_tabs.iter().map(|tab| {
                let select_id = tab.id.clone();
                let close_id = tab.id.clone();
                div()
                    .id(ElementId::Name(
                        format!("desktop-content-tab-{}", tab.id).into(),
                    ))
                    .flex()
                    .items_center()
                    .flex_none()
                    .rounded(px(9.0))
                    .bg(rgba(with_alpha(
                        theme.graphite,
                        if tab.selected { 0xcc } else { 0x5a },
                    )))
                    .child(
                        Button::new(ElementId::Name(
                            format!("desktop-content-select-{}", tab.id).into(),
                        ))
                        .label(tab.title.clone())
                        .custom(glass_button_variant(theme, cx))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.select_desktop_content_tab(&select_id, cx);
                        })),
                    )
                    .child(
                        Button::new(ElementId::Name(
                            format!("desktop-content-close-{}", tab.id).into(),
                        ))
                        .label("×")
                        .custom(glass_button_variant(theme, cx))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.close_desktop_content_tab(&close_id, cx);
                        })),
                    )
            }))
    }
}
