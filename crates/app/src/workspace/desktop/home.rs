use crate::app::MultisoulApp;
use canvas::CanvasRect;
use gpui::{
    div, img, prelude::*, px, rgba, Context, Div, Entity, ObjectFit, Stateful, Window,
    WindowControlArea,
};
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    input::{Input, InputState},
    Disableable, Icon, IconName, StyledExt,
};
use ui::icons::{toolbar_icon, with_alpha, ToolbarIcon};

// Popovers need a denser reading surface: 255 × 0.94 ≈ 240.
pub(super) const POPOVER_ALPHA: u8 = 240;

pub(crate) fn home_theme(id: ui::ThemeId) -> ui::Theme {
    let mut theme = crate::app::top_bar_theme(ui::Theme::for_id(id));
    theme.dock = 0x080d14ed;
    theme.dock_line = 0xa0afbc4a;
    theme.panel = 0x0d131be0;
    theme.panel_high = 0x1c2732eb;
    theme.node_fill = 0x253442f2;
    theme.canvas_from = theme.panel;
    theme.canvas_to = 0x080d15db;
    theme.node_stroke = 0xa3b2bf59;
    theme.edge_highlight = 0xb5c4cf88;
    theme.focused_stroke = 0x89bde3ff;
    theme.text_secondary = 0xcbd4dee8;
    theme.text_tertiary = 0xb1becbc8;
    theme.accent = 0x2b6aa5ff;
    theme
}

pub(crate) fn home_primary_button(theme: ui::Theme, cx: &gpui::App) -> ButtonCustomVariant {
    let accent = rgba(theme.accent);
    ButtonCustomVariant::new(cx)
        .color(accent.into())
        .foreground(rgba(0xffffffff).into())
        .border(rgba(with_alpha(theme.text_primary, 0x18)).into())
        .hover(accent.blend(rgba(0x00000018)).into())
        .active(accent.blend(rgba(0x00000038)).into())
}

pub(super) fn home_secondary_button(theme: ui::Theme, cx: &gpui::App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(rgba(0x00000000).into())
        .foreground(rgba(theme.text_primary).into())
        .border(rgba(0x00000000).into())
        .hover(rgba(with_alpha(theme.text_primary, 0x24)).into())
        .active(rgba(with_alpha(theme.text_primary, 0x33)).into())
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_home_material_tests.rs"]
mod material_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_home_background_tests.rs"]
mod background_tests;

impl MultisoulApp {
    pub(super) fn render_desktop_home(
        &mut self,
        input: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = home_theme(self.theme_id);
        let viewport = window.viewport_size();
        let full = CanvasRect::new(0.0, 0.0, viewport.width.into(), viewport.height.into());
        let first_workspace = self.desktop_workspace.workspaces.is_empty();
        let toolbar_height = if full.size.x < 640.0 { 104.0 } else { 52.0 };
        let frame = self.desktop_scene_frame(full, true);
        let scale = frame.size.x / full.size.x;
        let moving = self.desktop_directory_zoom_active();
        let chrome_opacity = self
            .desktop_directory_workspace_id()
            .and_then(|id| self.desktop_workspace.workspace_motion_rects.get(id))
            .map_or(1.0, |source| {
                let workspace_frame = self
                    .desktop_workspace
                    .transition
                    .as_ref()
                    .filter(|t| t.directory_content.is_none())
                    .map_or_else(|| self.desktop_scene_frame(full, false), |t| t.rect());
                super::scene::overview_chrome_opacity(workspace_frame, *source, full)
            });
        let workspaces: Vec<_> = self
            .desktop_workspace
            .filtered()
            .into_iter()
            .cloned()
            .collect();
        let layout = super::layout::home_layout(full.size.x, full.size.y, workspaces.len());
        let scroll = self.desktop_directory_scroll.offset();
        let content_height = layout.last().map_or(350.0, |r| r.max().y + 32.0);
        let left = layout.first().map_or(48.0, |r| r.origin.x);
        let heading_y = if full.size.x >= 640.0 { 86.0 } else { 136.0 };
        let count = self.desktop_workspace.workspaces.len();
        let query = !self.desktop_workspace.search_query.trim().is_empty();
        let heading = |text: String, y: f32, size: f32, color: u32| {
            div()
                .absolute()
                .left(px(left * scale))
                .top(px(y * scale))
                .text_size(px(size * scale))
                .line_height(px(size * if size >= 30.0 { 1.1 } else { 1.3 } * scale))
                .text_color(rgba(color))
                .font_semibold()
                .when(size >= 30.0, |heading| heading.font_bold())
                .child(text)
        };
        let mut content = div()
            .relative()
            .w(px(full.size.x * scale))
            .h(px(content_height * scale))
            .when(!first_workspace, |content| {
                content.child(
                    div()
                        .debug_selector(|| "desktop-home-heading".to_string())
                        .absolute()
                        .left(px(left * scale))
                        .top(px(heading_y * scale))
                        .max_w(px((full.size.x - 2.0 * left + 24.0) * scale))
                        .flex()
                        .flex_col()
                        .gap(px(2.0 * scale))
                        .child(
                            div()
                                .text_size(px(38.0 * scale))
                                .line_height(px(41.8 * scale))
                                .font_bold()
                                .child(ui::i18n::text("workspace.workspaces")),
                        )
                        .child(
                            div()
                                .text_size(px(18.0 * scale))
                                .line_height(px(23.4 * scale))
                                .font_semibold()
                                .text_color(rgba(theme.text_secondary))
                                .child(if query {
                                    ui::i18n::format(
                                        "workspace.of_count_workspaces",
                                        &[
                                            ("arg0", (workspaces.len()).to_string()),
                                            ("count", (count).to_string()),
                                        ],
                                    )
                                } else {
                                    ui::i18n::format(
                                        "workspace.count_workspaces",
                                        &[("count", (count).to_string())],
                                    )
                                }),
                        ),
                )
            })
            .child(heading(
                if query {
                    ui::i18n::text("workspace.search_results")
                } else {
                    ui::i18n::text("workspace.continue_working")
                }
                .into(),
                heading_y + 86.0,
                16.0,
                theme.text_secondary,
            ));
        if let Some(other) = layout.get(3) {
            content = content.child(heading(
                ui::i18n::text("workspace.other_workspaces").into(),
                other.origin.y - 32.0,
                16.0,
                theme.text_secondary,
            ));
        }
        if workspaces.is_empty() && !first_workspace {
            content = content.child(
                div()
                    .absolute()
                    .top(px((heading_y + 150.0) * scale))
                    .left(px(left * scale))
                    .right(px(left * scale))
                    .p(px(12.0 * scale))
                    .rounded(px(8.0 * scale))
                    .bg(rgba(theme.panel))
                    .text_size(px(15.0 * scale))
                    .text_color(rgba(theme.text_secondary))
                    .child(ui::i18n::text("workspace.no_workspaces_match_your_search")),
            );
        }
        content = content.children(workspaces.into_iter().zip(layout).enumerate().map(
            |(index, (workspace, rect))| {
                let mut preview = super::layout::home_preview_panel_rect(rect, index == 0);
                preview.origin.x += 1.0;
                preview.origin.y += 1.0;
                if !moving {
                    let mut anchor = preview;
                    anchor.origin.y += f32::from(scroll.y);
                    self.desktop_workspace
                        .workspace_rects
                        .lock()
                        .expect("workspace bounds mutex poisoned")
                        .insert(workspace.id.clone(), anchor);
                }
                div()
                    .absolute()
                    .left(px(rect.origin.x * scale))
                    .top(px(rect.origin.y * scale))
                    .w(px(rect.size.x * scale))
                    .h(px(rect.size.y * scale))
                    .child(
                        self.desktop_home_card(index, workspace, rect.size, full.size, scale, cx),
                    )
            },
        ));
        let mut page = div()
            .id("desktop-home-world")
            .absolute()
            .left(px(frame.origin.x))
            .top(px(frame.origin.y))
            .w(px(frame.size.x))
            .h(px(frame.size.y))
            .text_color(rgba(theme.text_primary));
        page = if moving {
            page.child(
                div()
                    .absolute()
                    .top(px(f32::from(scroll.y) * scale))
                    .child(content),
            )
        } else if first_workspace {
            page.child(self.desktop_home_welcome(full.size, cx))
                .child(self.desktop_home_toolbar(input, full.size.x, cx))
        } else {
            page.child(
                div()
                    .id("desktop-workspace-scroll")
                    .debug_selector(|| "desktop-workspace-scroll".to_string())
                    .absolute()
                    .inset_0()
                    .top(px(toolbar_height))
                    .overflow_y_scroll()
                    .track_scroll(&self.desktop_directory_scroll)
                    .child(
                        content
                            .mt(px(-toolbar_height))
                            .h(px(content_height - toolbar_height)),
                    ),
            )
            .child(self.desktop_home_toolbar(input, full.size.x, cx))
        };
        page = page.when_some(self.desktop_workspace.error.clone(), |page, error| {
            page.child(
                div()
                    .absolute()
                    .bottom(px(12.0))
                    .left(px(left))
                    .right(px(left))
                    .p(px(12.0))
                    .rounded(px(8.0))
                    .bg(rgba(with_alpha(theme.panel_high, POPOVER_ALPHA)))
                    .text_size(px(13.0))
                    .text_color(rgba(theme.destructive))
                    .child(error),
            )
        });
        div()
            .id("desktop-workspace-launchpad")
            .debug_selector(|| "desktop-workspace-launchpad".to_string())
            .absolute()
            .inset_0()
            .when(!moving, |root| {
                root.child(
                    div()
                        .absolute()
                        .inset_0()
                        .debug_selector(|| "desktop-home-wallpaper".to_string())
                        .bg(rgba(0x030915ff))
                        .child(
                            img(crate::assets::HOME_BACKGROUND_ASSET)
                                .absolute()
                                .inset_0()
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        ),
                )
            })
            .child(page.opacity(chrome_opacity))
    }

    fn desktop_home_welcome(&self, size: canvas::Vec2, cx: &mut Context<Self>) -> Stateful<Div> {
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

    fn desktop_home_toolbar(
        &self,
        input: Entity<InputState>,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = home_theme(self.theme_id);
        let has_workspaces = !self.desktop_workspace.workspaces.is_empty();
        let narrow = width < 640.0 && has_workspaces;
        let search = || {
            div()
                .id("desktop-workspace-search")
                .debug_selector(|| "desktop-workspace-search".to_string())
                .w(px(if narrow {
                    (width - 40.0).max(1.0)
                } else {
                    (width * 0.20).clamp(180.0, 244.0)
                }))
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(8.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.text_primary, 0x28)))
                .bg(rgba(with_alpha(theme.panel, 0xf0)))
                .child(
                    Input::new(&input)
                        .appearance(false)
                        .cleanable(true)
                        .prefix(Icon::new(IconName::Search).text_color(rgba(theme.text_secondary))),
                )
        };
        div()
            .debug_selector(|| "desktop-home-toolbar".to_string())
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .h(px(if narrow { 104.0 } else { 52.0 }))
            .bg(rgba(theme.dock))
            .text_color(rgba(theme.text_primary))
            .border_b_1()
            .border_color(rgba(theme.dock_line))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(
                div()
                    .h(px(52.0))
                    .pl(px(108.0))
                    .pr(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(div().text_size(px(17.0)).font_semibold().child("Norma"))
                    .child(div().flex_1())
                    .when(!narrow && has_workspaces, |row| row.child(search()))
                    .when(has_workspaces, |row| {
                        row.child(
                            Button::new("desktop-workspace-create")
                                .debug_selector(|| "desktop-workspace-create".to_string())
                                .label(ui::i18n::text("workspace.new_workspace"))
                                .text_size(px(14.0))
                                .h(px(36.0))
                                .custom(home_primary_button(theme, cx))
                                .disabled(self.desktop_workspace.busy)
                                .on_click(cx.listener(|app, _, window, cx| {
                                    app.create_desktop_workspace(window, cx)
                                })),
                        )
                    }),
            )
            .when(narrow, |bar| {
                bar.child(
                    div()
                        .absolute()
                        .top(px(58.0))
                        .left(px(20.0))
                        .child(search()),
                )
            })
    }
}
