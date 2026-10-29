//! Full-width workspace title bar sharing the desktop surface theme.

use crate::app::MultisoulApp;
use crate::shortcuts::ShortcutCommandId;
use crate::workspace::persistence::PersistenceSaveState;
use gpui::{div, img, prelude::*, px, rgba, Context, Div, Stateful, Window, WindowControlArea};
use std::path::Path;
use ui::component::{
    button::{Button, ButtonCustomVariant, ButtonVariants},
    Disableable, StyledExt,
};
use ui::icons::{toolbar_icon, with_alpha, ToolbarIcon};
use workspace::display_name_for_path;

pub(crate) const TOP_BAR_HEIGHT: f32 = 52.0;
/// Left drawer toggle starts here in both drawer states (clears the traffic lights).
pub(crate) const BRAND_TOGGLE_LEFT: f32 = 82.0;
pub(crate) const BRAND_LOGO_SIZE: f32 = 22.0;
// toggle 32px + 10px gap after BRAND_TOGGLE_LEFT.
pub(crate) const BRAND_LOGO_LEFT: f32 = BRAND_TOGGLE_LEFT + 32.0 + 10.0;
// logo right edge + 12px gap.
const BRAND_BREADCRUMB_LEFT: f32 = BRAND_LOGO_LEFT + BRAND_LOGO_SIZE + 12.0;

pub(crate) fn brand_logo() -> Div {
    div()
        .debug_selector(|| "desktop-brand-logo".to_string())
        .w(px(BRAND_LOGO_SIZE))
        .h(px(BRAND_LOGO_SIZE))
        .flex_none()
        .child(
            img(crate::assets::APP_LOGO_ASSET)
                .w(px(BRAND_LOGO_SIZE))
                .h(px(BRAND_LOGO_SIZE)),
        )
}

pub(crate) fn top_bar_theme(theme: ui::Theme) -> ui::Theme {
    crate::workspace::desktop::surface_theme::surface_theme(theme)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CanvasSaveVisual {
    pub(super) label: &'static str,
    pub(super) dot: u32,
}

pub(super) fn canvas_save_visual(pending: bool, failed: bool) -> CanvasSaveVisual {
    if failed {
        CanvasSaveVisual {
            label: ui::i18n::text("chrome.save_issue"),
            dot: 0xff6b7aff,
        }
    } else if pending {
        CanvasSaveVisual {
            label: ui::i18n::text("cards.saving"),
            dot: 0xd6ff3bff,
        }
    } else {
        CanvasSaveVisual {
            label: ui::i18n::text("chrome.canvas_saved"),
            dot: 0x64e0b5ff,
        }
    }
}

impl MultisoulApp {
    pub(super) fn render_workspace_top_bar(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = top_bar_theme(self.theme);
        let project_name = workspace_top_bar_title(self.workspace_state.current_root.as_deref());
        let save = save_visual(self.persistence.save_state());

        top_bar_frame("workspace-top-bar", theme)
            .child(workspace_title(project_name, theme))
            .child(self.workspace_top_bar_actions(save, theme, cx))
    }

    pub(crate) fn render_desktop_workspace_top_bar(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = top_bar_theme(self.theme);
        let layout = self.desktop_assistant_layout(window);
        let width = layout.canvas_width;
        let wide = width >= 780.0;
        let workspace = self
            .desktop_workspace
            .active()
            .map(|item| item.name.clone())
            .unwrap_or_default();
        let project = self
            .desktop_workspace
            .expanded_area_id
            .as_deref()
            .and_then(|id| {
                self.desktop_workspace
                    .areas
                    .iter()
                    .find(|area| area.id == id)
            })
            .map(|area| area.display_name.clone());
        let beside_left_drawer = layout.left_space > 0.0;
        // The prototype anchors the breadcrumb to the canvas edge while the chat drawer
        // is open. Overlay drawers on narrow windows leave the title centered.
        let reserve = if wide { 220.0 } else { 104.0 };
        // With the drawer collapsed the logo stays at its drawer-header x and the
        // breadcrumb hugs it instead of centering.
        let brand_anchored = !self.desktop_workspace.drawers.left_open;
        let title_width = if beside_left_drawer {
            (width - if wide { 250.0 } else { 74.0 }).max(1.0)
        } else if brand_anchored {
            (width - BRAND_BREADCRUMB_LEFT - reserve).max(1.0)
        } else {
            (width - reserve * 2.0).max(1.0)
        };
        let title = desktop_top_bar_title(workspace, project, title_width, theme)
            .when(beside_left_drawer, |title| title.absolute().left(px(14.0)))
            .when(brand_anchored && !beside_left_drawer, |title| {
                title.absolute().left(px(BRAND_BREADCRUMB_LEFT))
            });
        top_bar_frame("desktop-workspace-top-bar", theme)
            .left(px(layout.left_space))
            .right(px(layout.right_space))
            .when(brand_anchored, |bar| {
                bar.child(
                    brand_logo()
                        .absolute()
                        .left(px(BRAND_LOGO_LEFT))
                        .top(px((TOP_BAR_HEIGHT - BRAND_LOGO_SIZE) / 2.0)),
                )
            })
            .child(title)
            .child(
                div()
                    .absolute()
                    .right(px(if self.desktop_workspace.drawers.right_open {
                        if wide {
                            24.0
                        } else {
                            12.0
                        }
                    } else if wide {
                        // Keep a small gap before the icon-only content drawer trigger.
                        58.0
                    } else {
                        58.0
                    }))
                    .h(px(TOP_BAR_HEIGHT))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        launchpad_button(theme, cx)
                            .tooltip(ui::i18n::text("chrome.all_workspaces"))
                            .disabled(self.desktop_workspace.busy)
                            .on_click(cx.listener(|app, _, window, cx| {
                                cx.stop_propagation();
                                app.navigate_desktop_shortcut(
                                    ShortcutCommandId::ShowWorkspaceDirectory,
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .when(self.desktop_workspace.expanded_area_id.is_none(), |row| {
                        row.child(
                            Button::new("desktop-topbar-add-project")
                                .debug_selector(|| "desktop-topbar-add-project".to_string())
                                .child(toolbar_icon(ToolbarIcon::Plus, theme.text_primary, 14.0))
                                .when(wide, |button| {
                                    button.label(ui::i18n::text("shortcuts.add_project"))
                                })
                                .when(!wide, |button| button.w(px(36.0)).p(px(0.0)))
                                .h(px(36.0))
                                .rounded(px(12.0))
                                .custom(top_bar_button(theme, cx))
                                .tooltip(ui::i18n::text("shortcuts.add_project"))
                                .disabled(self.desktop_workspace.busy)
                                .on_click(cx.listener(|app, _, window, cx| {
                                    cx.stop_propagation();
                                    app.open_desktop_add_project(window, cx);
                                })),
                        )
                    })
                    .when_some(self.render_status_capsule(window, cx), |row, capsule| {
                        row.child(capsule)
                    }),
            )
    }

    fn workspace_top_bar_actions(
        &mut self,
        save: CanvasSaveVisual,
        theme: ui::Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .absolute()
            .top(px(0.0))
            .right(px(18.0))
            .h(px(TOP_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                launchpad_button(theme, cx)
                    .tooltip(ui::i18n::text("chrome.all_workspaces"))
                    .on_click(cx.listener(|app, _, _window, cx| {
                        cx.stop_propagation();
                        app.open_new_launchpad_window(cx);
                    })),
            )
            .child(save_status(save, theme))
    }
}

fn save_visual(state: PersistenceSaveState) -> CanvasSaveVisual {
    match state {
        PersistenceSaveState::Clean => canvas_save_visual(false, false),
        PersistenceSaveState::Pending => canvas_save_visual(true, false),
        PersistenceSaveState::Failed => canvas_save_visual(false, true),
    }
}

fn top_bar_frame(id: &'static str, theme: ui::Theme) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .h(px(TOP_BAR_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .border_b_1()
        .border_color(rgba(theme.dock_line))
        .bg(rgba(theme.dock))
        .text_color(rgba(theme.text_primary))
        .child(
            div()
                .absolute()
                .inset_0()
                .window_control_area(WindowControlArea::Drag),
        )
}

fn desktop_top_bar_title(
    workspace: String,
    project: Option<String>,
    max_width: f32,
    theme: ui::Theme,
) -> Stateful<Div> {
    let workspace_selector = format!("workspace-topbar-workspace-{workspace}");
    let project_selector = project
        .as_ref()
        .map(|name| format!("workspace-topbar-project-{name}"));
    // 16px divider plus two 10px gaps = 36px; each label gets half the remaining width.
    let label_width = ((max_width - 36.0) * 0.5).max(1.0);
    div()
        .id("workspace-topbar-title")
        .debug_selector(|| "workspace-topbar-title".to_string())
        .max_w(px(max_width))
        .flex()
        .items_center()
        .gap(px(10.0))
        .text_size(px(14.0))
        .child(
            div()
                .debug_selector(move || workspace_selector.clone())
                .max_w(px(label_width))
                .truncate()
                .text_color(rgba(theme.text_secondary))
                .child(workspace),
        )
        .child(
            div()
                .w(px(16.0))
                .flex_none()
                .text_center()
                .text_color(rgba(theme.text_tertiary))
                .child("/"),
        )
        .child(
            div()
                .debug_selector(move || {
                    project_selector
                        .clone()
                        .unwrap_or_else(|| "workspace-topbar-projects".to_string())
                })
                .max_w(px(label_width))
                .truncate()
                .font_semibold()
                .text_color(rgba(theme.text_primary))
                .child(
                    project.unwrap_or_else(|| ui::i18n::text("workspace.all_projects").to_string()),
                ),
        )
}

fn workspace_top_bar_title(path: Option<&Path>) -> String {
    path.map(display_name_for_path).unwrap_or_default()
}

fn workspace_title(path: String, theme: ui::Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .child(
            div()
                .font_semibold()
                .text_color(rgba(theme.text_primary))
                .child("Norma OS"),
        )
        .child(
            div()
                .max_w(px(360.0))
                .overflow_hidden()
                .truncate()
                .font_family("Menlo")
                .text_size(px(9.0))
                .text_color(rgba(theme.text_secondary))
                .child(path),
        )
}

fn launchpad_button(theme: ui::Theme, cx: &gpui::App) -> Button {
    Button::new("workspace-launchpad-button")
        .debug_selector(|| "workspace-launchpad-button".to_string())
        .w(px(36.0))
        .h(px(36.0))
        .rounded(px(12.0))
        .custom(top_bar_button(theme, cx))
        .child(launchpad_grid_glyph(theme.text_primary))
}

fn top_bar_button(theme: ui::Theme, cx: &gpui::App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(rgba(with_alpha(theme.text_primary, 0x0c)).into())
        .foreground(rgba(theme.text_primary).into())
        .border(rgba(with_alpha(theme.text_primary, 0x20)).into())
        .hover(rgba(theme.panel_high).into())
        .active(rgba(theme.node_fill).into())
}

fn launchpad_grid_glyph(color: u32) -> Div {
    div()
        .grid()
        .grid_cols(3)
        .grid_rows(3)
        .gap(px(2.0))
        .children((0..9).map(|_| div().w(px(2.0)).h(px(2.0)).rounded(px(1.0)).bg(rgba(color))))
}

fn save_status(save: CanvasSaveVisual, theme: ui::Theme) -> Stateful<Div> {
    div()
        .id("workspace-save-status")
        .debug_selector(|| "workspace-save-status".to_string())
        .h(px(28.0))
        .flex()
        .items_center()
        .gap(px(7.0))
        .px(px(10.0))
        .rounded(px(14.0))
        .bg(rgba(with_alpha(theme.text_primary, 0x0c)))
        .text_color(rgba(theme.text_secondary))
        .text_size(px(10.0))
        .font_semibold()
        .child(
            div()
                .w(px(6.0))
                .h(px(6.0))
                .rounded(px(3.0))
                .bg(rgba(save.dot)),
        )
        .child(save.label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 顶部工作区标题：绝对工作区根目录只显示末级项目名称。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   workspace root = `/Users/alan/Documents/mybook/multisoul-desktop`
    ///   路径层级数 = 5（Users / alan / Documents / mybook / multisoul-desktop）
    ///   project name = 最末级目录 `multisoul-desktop`
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 将完整绝对工作区路径传入顶部栏标题的展示边界。
    ///   2. 展示边界提取路径的末级目录名，供 GPUI 标题元素渲染。
    ///
    /// 预期结果：
    ///   - 正断言：标题为 `multisoul-desktop`，让用户识别当前项目。
    ///   - 负断言：标题不应保留 `/Users/alan/Documents/mybook/` 等绝对路径前缀。
    #[test]
    fn test_workspace_top_bar_title_uses_project_name_not_absolute_path() {
        let workspace_root = Path::new("/Users/alan/Documents/mybook/multisoul-desktop");
        let title = workspace_top_bar_title(Some(workspace_root));

        assert_eq!(
            title, "multisoul-desktop",
            "顶部栏应只显示工作区的末级项目名称"
        );
        assert_ne!(
            title, "/Users/alan/Documents/mybook/multisoul-desktop",
            "顶部栏不应泄露完整绝对工作区路径"
        );
    }
}
