use super::*;
use gpui::{img, linear_color_stop, linear_gradient, WindowControlArea};
use ui::component::{Disableable, StyledExt};
use ui::icons::with_alpha;

impl MultisoulApp {
    pub(super) fn desktop_home_toolbar(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let theme = home_theme(self.theme_id);
        div()
            .debug_selector(|| "desktop-home-toolbar".into())
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(52.0))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(
                div()
                    .size_full()
                    .pl(px(if width < 500.0 { 86.0 } else { 108.0 }))
                    .pr(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(img(crate::assets::APP_LOGO_ASSET).size(px(26.0)))
                    .when(width >= 420.0, |row| {
                        row.child(div().text_size(px(15.0)).font_semibold().child("Norma"))
                    })
                    .child(div().flex_1())
                    .when(!self.desktop_workspace.workspaces.is_empty(), |row| {
                        row.child(
                            material::button(
                                "desktop-workspace-create",
                                ui::i18n::text("workspace.new_workspace"),
                                theme,
                                cx,
                            )
                            .debug_selector(|| "desktop-workspace-create".into())
                            .tab_stop(!self.desktop_workspace.home.search_open)
                            .h(px(36.0))
                            .bg(rgba(with_alpha(theme.text_primary, 20)))
                            .disabled(self.desktop_workspace.busy)
                            .opacity(if self.desktop_workspace.busy {
                                0.45
                            } else {
                                1.0
                            })
                            .on_click(cx.listener(
                                |app, _, window, cx| app.create_desktop_workspace(window, cx),
                            )),
                        )
                    }),
            )
    }

    pub(super) fn desktop_home_brand(
        &self,
        width: f32,
        top: f32,
        opacity: f32,
        cx: &gpui::App,
    ) -> Div {
        let theme = home_theme(self.theme_id);
        let greeting = if self.desktop_workspace.home.search_open {
            ui::i18n::text("workspace.home_search_greeting").to_string()
        } else if let Some(entry) = self
            .home_entries(cx)
            .into_iter()
            .find(|entry| entry.running)
        {
            ui::i18n::format("workspace.home_running_greeting", &[("name", entry.name)]).to_string()
        } else {
            ui::i18n::text("workspace.home_greeting").to_string()
        };
        let compact = width < 640.0;
        div()
            .debug_selector(|| "desktop-home-brand".into())
            .absolute()
            .left(px(20.0))
            .top(px(top))
            .w(px((width - 40.0).max(1.0)))
            .h(px(76.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(if compact { 12.0 } else { 16.0 }))
            .opacity(opacity)
            .child(
                div()
                    .relative()
                    .flex_none()
                    .size(px(if compact { 60.0 } else { 76.0 }))
                    .rounded_full()
                    .p(px(3.0))
                    .bg(linear_gradient(
                        140.0,
                        linear_color_stop(rgba(0x8e7cf8ff), 0.0),
                        linear_color_stop(rgba(0x6fd3e8ff), 1.0),
                    ))
                    .shadow(vec![gpui::BoxShadow {
                        color: rgba(0x8e7cf824).into(),
                        offset: gpui::point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(6.0),
                    }])
                    .child(
                        img(crate::assets::NORMA_CALM_ASSET)
                            .size_full()
                            .rounded_full()
                            .object_fit(gpui::ObjectFit::Cover),
                    )
                    .child(
                        div()
                            .absolute()
                            .right(px(1.0))
                            .bottom(px(3.0))
                            .size(px(16.0))
                            .rounded_full()
                            .border_3()
                            .border_color(rgba(0x0b1018ff))
                            .bg(rgba(theme.success)),
                    ),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .max_w(px(if compact { width - 124.0 } else { 560.0 }))
                    .px(px(if compact { 14.0 } else { 22.0 }))
                    .py(px(12.0))
                    .rounded(px(22.0))
                    .rounded_bl(px(6.0))
                    .border_1()
                    .border_color(rgba(0xffffff24))
                    .bg(rgba(0x161c24a8))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgba(0xb9b0faff))
                            .font_medium()
                            .child("Norma"),
                    )
                    .child(
                        div()
                            .text_size(px(if compact { 15.0 } else { 20.0 }))
                            .line_height(px(28.0))
                            .text_color(rgba(theme.text_primary))
                            .child(greeting),
                    ),
            )
    }
}
