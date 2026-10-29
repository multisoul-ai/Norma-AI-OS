use super::*;
use ui::component::{
    button::{Button, ButtonVariants},
    Disableable, StyledExt,
};
use ui::icons::{toolbar_icon, with_alpha, ToolbarIcon};

impl MultisoulApp {
    pub(super) fn desktop_home_welcome(
        &self,
        size: canvas::Vec2,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = home_theme(self.theme_id);
        let compact = size.x < 640.0 || size.y < 600.0;
        div()
            .id("desktop-home-welcome-area")
            .absolute()
            .inset_0()
            .top(px(52.0))
            .p(px(24.0))
            .flex()
            .flex_col()
            .items_center()
            .overflow_y_scroll()
            .child(
                div()
                    .debug_selector(|| "desktop-home-welcome".into())
                    .w_full()
                    .max_w(px(480.0))
                    .flex_none()
                    // Auto margins center the panel while allowing short windows to scroll.
                    .my(gpui::auto())
                    .p(px(if compact { 24.0 } else { 40.0 }))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(20.0))
                    .rounded(px(20.0))
                    .border_1()
                    .border_color(rgba(with_alpha(theme.text_primary, 0x20)))
                    .bg(rgba(theme.panel))
                    .shadow_md()
                    .child(
                        div()
                            .size(px(if compact { 48.0 } else { 64.0 }))
                            .flex_none()
                            .rounded(px(14.0))
                            .border_1()
                            .border_color(rgba(with_alpha(theme.accent, 0x30)))
                            .bg(rgba(with_alpha(theme.accent, 0x18)))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(toolbar_icon(ToolbarIcon::Workspaces, theme.accent, 32.0)),
                    )
                    .child(
                        div()
                            .debug_selector(|| "desktop-home-welcome-title".into())
                            .w_full()
                            .text_center()
                            .text_size(px(if compact { 22.0 } else { 26.0 }))
                            .line_height(px(if compact { 28.0 } else { 32.0 }))
                            .font_semibold()
                            .text_color(rgba(theme.text_primary))
                            .child(ui::i18n::text("workspace.create_your_first_workspace")),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(px(14.0))
                            .line_height(px(22.0))
                            .text_color(rgba(theme.text_primary))
                            .child(ui::i18n::text(
                                "workspace.bring_your_projects_and_agents_together_on_one_canvas",
                            )),
                    )
                    .child(
                        Button::new("desktop-workspace-create")
                            .debug_selector(|| "desktop-workspace-create".into())
                            .label(ui::i18n::text("workspace.create_workspace"))
                            .text_size(px(14.0))
                            .h(px(40.0))
                            .primary()
                            .disabled(self.desktop_workspace.busy)
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.create_desktop_workspace(window, cx)
                            })),
                    ),
            )
    }
}
