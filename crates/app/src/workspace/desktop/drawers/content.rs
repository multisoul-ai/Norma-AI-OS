//! The right content drawer and its workspace-scoped tabs.

use super::{BOTTOM, GLASS_TEXT, GLASS_TEXT_SECONDARY, TOP};
use crate::app::MultisoulApp;
use gpui::{
    div, point, prelude::*, px, rgba, BoxShadow, Context, Div, MouseButton, Stateful, Window,
};
use ui::icons::with_alpha;

pub(crate) mod browser_toolbar;
mod chrome;
mod chrome_controls;
mod chrome_format;
mod home;
mod home_tools;
mod preview_load;
mod render;
mod tabs;
pub(crate) mod workbench;

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_content_tab_order_tests.rs"]
mod tests;

impl MultisoulApp {
    pub(super) fn render_assistant_right(
        &mut self,
        width: f32,
        edge_inset: f32,
        opacity: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let frost = self.render_drawer_frost(false, width, edge_inset, window);
        let selected = self
            .desktop_workspace
            .content_tabs
            .iter()
            .find(|tab| tab.selected)
            .cloned();
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
            .child(self.render_reader_chrome(cx))
            .when(
                selected
                    .as_ref()
                    .is_some_and(|tab| tab.kind != storage::DesktopContentKind::ProjectFiles),
                |drawer| drawer.child(self.render_reader_toolbar(cx)),
            )
            .child(
                div()
                    .id("desktop-content-body")
                    .debug_selector(|| "desktop-content-body".to_string())
                    .flex_1()
                    .min_h(px(0.0))
                    .when_some(selected, |body, tab| {
                        body.child(self.render_content_tab_body(&tab, width, window, cx))
                    })
                    .when(self.desktop_workspace.content_tabs.is_empty(), |body| {
                        body.child(self.render_content_home(width, window, cx))
                    }),
            )
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_reader_chrome_format_tests.rs"]
mod reader_chrome_format_tests;

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_content_home_surface_tests.rs"]
mod home_surface_tests;
