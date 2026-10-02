use crate::app::MultisoulApp;
use canvas::CanvasRect;
use gpui::{
    div, linear_color_stop, linear_gradient, prelude::*, px, rgba, Context, Div, Entity, Stateful,
    Window, WindowControlArea,
};
use ui::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    Disableable, Icon, IconName, StyledExt,
};
use ui::icons::{toolbar_icon, with_alpha, ToolbarIcon};

// 80% transparency: 255 × (1 - 0.8) = 51. Text and live previews stay clear.
pub(super) const SURFACE_ALPHA: u8 = 51;

impl MultisoulApp {
    pub(super) fn render_desktop_home(
        &mut self,
        input: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
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
            .child(heading(
                "Workspaces".into(),
                heading_y,
                38.0,
                theme.text_primary,
            ))
            .child(heading(
                if query {
                    format!("{} of {count} workspaces", workspaces.len())
                } else {
                    format!("{count} workspaces")
                },
                heading_y + 44.0,
                18.0,
                theme.text_secondary,
            ))
            .child(heading(
                if query {
                    "Search results"
                } else {
                    "Continue working"
                }
                .into(),
                heading_y + 86.0,
                16.0,
                theme.text_secondary,
            ));
        if let Some(other) = layout.get(3) {
            content = content.child(heading(
                "Other workspaces".into(),
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
                    .text_size(px(15.0 * scale))
                    .text_color(rgba(theme.text_secondary))
                    .child("No workspaces match your search."),
            );
        }
        content = content.children(workspaces.into_iter().zip(layout).enumerate().map(
            |(index, (workspace, rect))| {
                let preview = super::layout::home_preview_rect(rect, full.size, index == 0);
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
                    .bg(rgba(with_alpha(theme.panel_high, SURFACE_ALPHA)))
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
                        .child(self.render_workspace_backdrop()),
                )
            })
            // The empty welcome page has no catalog text to protect with a full-window scrim.
            .when(!first_workspace, |root| {
                root.child(div().absolute().inset_0().bg(rgba(with_alpha(
                    theme.canvas_from,
                    (chrome_opacity * if self.theme_id.is_dark() { 36.0 } else { 128.0 }) as u8,
                ))))
            })
            .child(page.opacity(chrome_opacity))
    }

    fn desktop_home_welcome(&self, size: canvas::Vec2, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
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
                    // A single 6–13% tint preserves the wallpaper instead of stacking white fills.
                    .bg(linear_gradient(
                        180.0,
                        linear_color_stop(rgba(with_alpha(theme.panel_high, 0x20)), 0.0),
                        linear_color_stop(rgba(with_alpha(theme.panel, 0x10)), 1.0),
                    ))
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
                            .child("Create your first workspace"),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(px(14.0))
                            .line_height(px(22.0))
                            .text_color(rgba(theme.text_primary))
                            .child("Bring your projects and agents together on one canvas."),
                    )
                    .child(
                        Button::new("desktop-workspace-create")
                            .debug_selector(|| "desktop-workspace-create".into())
                            .label("Create Workspace")
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
        let theme = self.theme;
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
                .bg(rgba(with_alpha(theme.panel, SURFACE_ALPHA)))
                .child(
                    Input::new(&input)
                        .appearance(false)
                        .cleanable(true)
                        .prefix(Icon::new(IconName::Search).text_color(rgba(theme.text_secondary))),
                )
        };
        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .right(px(0.0))
            .h(px(if narrow { 104.0 } else { 52.0 }))
            .bg(rgba(with_alpha(
                theme.panel_high,
                if has_workspaces { SURFACE_ALPHA } else { 0x18 },
            )))
            .border_b_1()
            .border_color(rgba(with_alpha(theme.text_primary, 0x16)))
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
                                .label("+ New Workspace")
                                .text_size(px(14.0))
                                .h(px(36.0))
                                .primary()
                                .bg(rgba(with_alpha(theme.accent, SURFACE_ALPHA)))
                                .text_color(rgba(theme.text_primary))
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
