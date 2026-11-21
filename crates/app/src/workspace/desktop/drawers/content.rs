//! The right content drawer and its workspace-scoped tabs.

use super::{BOTTOM, TOP};
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
mod open;
mod preview_load;
pub(crate) mod refresh;
mod render;
pub(crate) mod tab_controls;
mod tab_search;
mod tab_strip;
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
        let (top, bottom) = (TOP, BOTTOM);
        let frost = Some(self.render_drawer_frost(false, width, edge_inset, window));
        let new_tab_page = self.desktop_workspace.content_browser.new_tab_page;
        let selected = self
            .desktop_workspace
            .content_tabs
            .iter()
            .find(|tab| tab.selected && !new_tab_page)
            .cloned();
        let confirm_batch = matches!(
            self.desktop_workspace.workbench.pending_close,
            Some(workbench::PendingWorkbenchAction::CloseTabs { .. })
        );
        let closing_selected = selected.as_ref().is_some_and(|tab| {
            self.content_tab_controls.closing.contains(&tab.id)
                && tab.kind == storage::DesktopContentKind::ProjectFiles
        });
        let menu = self.render_content_tab_menu();
        let search = self.render_content_tab_search(width, edge_inset, window, cx);
        div()
            .id("desktop-assistant-right")
            .debug_selector(|| "desktop-assistant-right".to_string())
            .absolute()
            .right(px(edge_inset))
            .top(px(top))
            .bottom(px(bottom))
            .w(px(width))
            .opacity(opacity)
            .flex()
            .flex_col()
            .rounded(px(0.0))
            .overflow_hidden()
            .border_l_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x55)))
            .shadow(vec![BoxShadow {
                color: rgba(crate::workspace::desktop::surface_theme::shadow_color(
                    theme, 0x70,
                ))
                .into(),
                offset: point(px(-14.0), px(0.0)),
                blur_radius: px(48.0),
                spread_radius: px(0.0),
            }])
            .text_color(rgba(theme.text_primary))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when_some(frost, |drawer, frost| drawer.child(frost))
            .child(self.render_reader_chrome(width, window, cx))
            .when_some(selected.as_ref(), |drawer, tab| {
                drawer.child(self.render_reader_toolbar(tab, width, edge_inset, window, cx))
            })
            .child(
                div()
                    .id("desktop-content-body")
                    .debug_selector(|| "desktop-content-body".to_string())
                    .flex_1()
                    .min_h(px(0.0))
                    .when(confirm_batch, |body| {
                        body.child(self.render_workbench_unsaved(cx))
                    })
                    .when(closing_selected, |body| {
                        body.child(
                            div()
                                .p(px(14.0))
                                .child(ui::i18n::text("drawers.closing_tabs")),
                        )
                    })
                    .when_some(
                        selected.filter(|_| !confirm_batch && !closing_selected),
                        |body, tab| {
                            body.child(self.render_content_tab_body(&tab, width, window, cx))
                        },
                    )
                    .when(
                        !confirm_batch
                            && (new_tab_page || self.desktop_workspace.content_tabs.is_empty()),
                        |body| body.child(self.render_content_home(width, window, cx)),
                    ),
            )
            .children(menu)
            .children(search)
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_reader_chrome_format_tests.rs"]
mod reader_chrome_format_tests;

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_content_home_surface_tests.rs"]
mod home_surface_tests;

#[cfg(test)]
#[path = "../../../../tests/unit/desktop_presentation_endpoint_tests.rs"]
mod presentation_endpoint_tests;
