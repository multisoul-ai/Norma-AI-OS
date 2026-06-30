//! Workspace launchpad UI.

mod actions;
mod backdrop;
mod layout;
mod preview;
mod recent;

use self::actions::{action_card, launchpad_brand};
use self::backdrop::{launchpad_backdrop, toolbar_preview};
use self::layout::launchpad_layout_for_window;
#[cfg(test)]
use self::layout::LaunchpadLayoutMode;
use self::preview::preview_cluster;
#[cfg(test)]
use self::recent::recent_project_records_for_launchpad;
use crate::app::MultisoulApp;
use gpui::{div, point, prelude::*, px, rgba, BoxShadow, ClickEvent, Context, Div, Window};
use ui::Theme;

impl MultisoulApp {
    pub(crate) fn render_launchpad(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let win = window.viewport_size();
        let layout = launchpad_layout_for_window(f32::from(win.width), f32::from(win.height));

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .overflow_hidden()
            .bg(rgba(theme.graphite))
            .child(launchpad_backdrop(
                theme,
                self.wallpaper_settings.image_source_for_render(),
            ))
            .child(toolbar_preview(theme, layout))
            .child(
                div()
                    .absolute()
                    .left(px(layout.content_left))
                    .top(px(layout.content_top))
                    .w(px(layout.content_width))
                    .flex()
                    .flex_col()
                    .gap(px(28.0))
                    .child(launchpad_brand(theme))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(14.0))
                            .child(
                                action_card(
                                    theme,
                                    "□",
                                    "Open Project",
                                    "Choose a local workspace",
                                    true,
                                )
                                .id("launchpad-open-project")
                                .on_click(cx.listener(Self::on_launchpad_open_project)),
                            )
                            .child(
                                action_card(theme, "↓", "Clone Repo", "Remote setup soon", false)
                                    .id("launchpad-clone-repo")
                                    .on_click(cx.listener(Self::on_launchpad_clone_todo)),
                            ),
                    )
                    .child(self.render_recent_projects(theme, cx)),
            )
            .when(layout.preview_visible, |this| {
                this.child(preview_cluster(theme, layout))
            })
    }

    fn on_launchpad_open_project(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt_open_project(cx);
    }

    fn on_launchpad_clone_todo(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_state.mark_clone_todo();
        cx.notify();
    }
}

pub(super) fn launchpad_shadow(theme: Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(theme.shadow_color).into(),
        offset: point(px(0.0), px(16.0)),
        blur_radius: px(34.0),
        spread_radius: px(-22.0),
    }]
}

pub(super) fn launchpad_logo_shadow(theme: Theme) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(with_alpha(theme.shadow_color, 0x8c)).into(),
        offset: point(px(0.0), px(12.0)),
        blur_radius: px(24.0),
        spread_radius: px(-14.0),
    }]
}

pub(super) fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
#[path = "../../../tests/unit/launchpad_tests.rs"]
mod tests;
