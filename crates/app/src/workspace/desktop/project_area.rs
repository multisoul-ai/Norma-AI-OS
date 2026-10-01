use super::{
    glass_button_variant,
    preview_canvas::{fitted_rects_at_scale, project_stage_rect, project_tone},
    GLASS_TEXT, GLASS_TEXT_SECONDARY,
};
use crate::app::MultisoulApp;
use canvas::CanvasNodeSnapshot;
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful, Window};
use storage::DesktopProjectArea;
use ui::component::button::{Button, ButtonVariants};
use ui::component::StyledExt;
use ui::icons::with_alpha;

impl MultisoulApp {
    pub(super) fn desktop_project_area_card(
        &self,
        area_index: usize,
        area: DesktopProjectArea,
        nodes: Vec<CanvasNodeSnapshot>,
        workspace_id: &str,
        card_width: f32,
        available_height: f32,
        scale: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let area_id = area.id.clone();
        let workspace_id = workspace_id.to_owned();
        let moving = self
            .desktop_workspace
            .transition
            .as_ref()
            .filter(|transition| {
                matches!(&transition.subject, super::camera::DesktopCanvasSubject::Project {
                workspace_id: active_workspace, area_id: active_area,
            } if active_workspace == &workspace_id && active_area == &area_id)
            });
        let shell_opacity = moving.map_or(1.0, |transition| {
            let source = self
                .desktop_workspace
                .area_rects
                .lock()
                .expect("project bounds mutex poisoned")
                .get(&(workspace_id.clone(), area_id.clone()))
                .copied();
            let size = window.viewport_size();
            source.map_or(0.0, |source| {
                super::scene::overview_chrome_opacity(
                    transition.rect(),
                    source,
                    super::desktop_scene_rect(f32::from(size.width), f32::from(size.height)),
                )
            })
        });
        let selected_node_id = self.desktop_workspace.enlarged_agent.as_ref().and_then(
            |(selected_workspace, selected_area, selected_node)| {
                (selected_workspace == &workspace_id && selected_area == &area_id)
                    .then_some(*selected_node)
            },
        );
        let card_height = available_height;
        let stage = project_stage_rect(canvas::Vec2::new(card_width, card_height), scale);
        let title_height = stage.origin.y;
        let stage_width = stage.size.x;
        let stage_height = stage.size.y;
        let rects = fitted_rects_at_scale(&nodes, stage_width, stage_height, scale);
        let mut positioned_nodes: Vec<_> = nodes.into_iter().zip(rects).enumerate().collect();
        positioned_nodes.sort_by_key(|(_, (node, _))| Some(node.id) == selected_node_id);
        let actions_open = self.desktop_workspace.actions_area_id.as_deref() == Some(&area_id);
        let click_area = area_id.clone();
        let mut card = div()
            .id(gpui::ElementId::Name(
                format!("desktop-project-area-{area_index}").into(),
            ))
            .debug_selector(move || format!("desktop-project-area-{area_index}"))
            .relative()
            .h(px(card_height))
            .rounded(px(14.0 * scale))
            .border_1()
            .border_color(rgba(theme.dock_line))
            .bg(rgba(with_alpha(theme.dock, 0x8c)))
            .opacity(shell_opacity)
            .cursor_pointer()
            .hover(move |style| style.border_color(rgba(theme.focused_stroke)))
            .active(move |style| style.bg(rgba(with_alpha(theme.dock, 0xb0))))
            .tab_index(0)
            .focus(move |style| style.border_color(rgba(theme.focused_stroke)))
            .on_click(cx.listener(move |app, _, window, cx| {
                app.expand_desktop_project_area(click_area.clone(), window, cx);
            }))
            .child(
                div()
                    .h(px(title_height))
                    .px(px(12.0 * scale))
                    .pr(px(52.0 * scale))
                    .flex()
                    .items_center()
                    .gap(px(7.0 * scale))
                    .text_color(rgba(GLASS_TEXT))
                    .child(
                        div()
                            .size(px(6.0 * scale))
                            .flex_none()
                            .rounded_full()
                            .bg(rgba(project_tone(&area.display_name, theme))),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("desktop-project-title-{area_index}"))
                            .flex_none()
                            .max_w(px(180.0 * scale))
                            .text_size(px(13.0 * scale))
                            .font_semibold()
                            .truncate()
                            .child(area.display_name.clone()),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("desktop-project-path-{area_index}"))
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(10.0 * scale))
                            .text_color(rgba(GLASS_TEXT_SECONDARY))
                            .truncate()
                            .child(workspace::compact_path(&area.root)),
                    ),
            )
            .when(moving.is_none(), |this| {
                this.child(
                    div()
                        .relative()
                        .w(px(stage_width))
                        .h(px(stage_height))
                        .mx(px(10.0 * scale))
                        .mb(px(10.0 * scale))
                        .when(positioned_nodes.is_empty(), |this| {
                            this.flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(12.0 * scale))
                                .text_color(rgba(GLASS_TEXT_SECONDARY))
                                .child(
                                    div()
                                        .debug_selector(move || {
                                            format!("desktop-project-empty-title-{area_index}")
                                        })
                                        .child("No cards yet"),
                                )
                        })
                        .children(positioned_nodes.into_iter().map(
                            |(node_index, (node, rect))| {
                                let is_selected = selected_node_id == Some(node.id);
                                let (x, y, width, height) = if is_selected {
                                    (0.0, 0.0, stage_width, stage_height)
                                } else {
                                    (rect.origin.x, rect.origin.y, rect.size.x, rect.size.y)
                                };
                                div().absolute().left(px(x)).top(px(y)).child(
                                    self.desktop_project_area_node(
                                        area_index,
                                        node_index,
                                        node,
                                        &workspace_id,
                                        &area_id,
                                        &area.display_name,
                                        width,
                                        height,
                                        is_selected,
                                        window,
                                        cx,
                                    ),
                                )
                            },
                        )),
                )
            });

        if self.desktop_workspace.transition.is_none() {
            let open_id = area_id.clone();
            let codex_id = area_id.clone();
            let claude_id = area_id.clone();
            let remove_id = area_id.clone();
            let actions_id = area_id.clone();
            card = card.child(
                div()
                    .absolute()
                    .top(px(8.0 * scale))
                    .right(px(8.0 * scale))
                    .child(
                        Button::new(("desktop-project-actions", area_index))
                            .debug_selector(move || format!("desktop-project-actions-{area_index}"))
                            .label("⋯")
                            .compact()
                            .w(px(28.0 * scale))
                            .h(px(28.0 * scale))
                            .text_size(px(14.0 * scale))
                            .custom(glass_button_variant(theme, cx))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                cx.stop_propagation();
                                app.desktop_workspace.actions_area_id =
                                    if app.desktop_workspace.actions_area_id.as_deref()
                                        == Some(actions_id.as_str())
                                    {
                                        None
                                    } else {
                                        Some(actions_id.clone())
                                    };
                                cx.notify();
                            })),
                    ),
            );
            if actions_open {
                card = card.child(
                    div()
                        .debug_selector(move || format!("desktop-project-menu-{area_index}"))
                        .absolute()
                        .top(px(46.0))
                        .right(px(8.0))
                        .w(px(116.0))
                        .p(px(5.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .rounded(px(9.0))
                        .border_1()
                        .border_color(rgba(theme.dock_line))
                        .bg(rgba(theme.dock))
                        .shadow_lg()
                        .child(
                            Button::new(("desktop-project-open", area_index))
                                .debug_selector(move || {
                                    format!("desktop-project-open-{area_index}")
                                })
                                .label("Open")
                                .compact()
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_area_id = None;
                                    app.expand_desktop_project_area(open_id.clone(), window, cx);
                                })),
                        )
                        .child(
                            Button::new(("desktop-project-new-codex", area_index))
                                .debug_selector(move || {
                                    format!("desktop-project-new-codex-{area_index}")
                                })
                                .label("New Codex")
                                .compact()
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_area_id = None;
                                    app.begin_desktop_agent_creation_for_area(
                                        codex_id.clone(),
                                        canvas::AgentProfile::Codex,
                                        None,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new(("desktop-project-new-claude", area_index))
                                .debug_selector(move || {
                                    format!("desktop-project-new-claude-{area_index}")
                                })
                                .label("New Claude")
                                .compact()
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_area_id = None;
                                    app.begin_desktop_agent_creation_for_area(
                                        claude_id.clone(),
                                        canvas::AgentProfile::Claude,
                                        None,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new(("desktop-project-remove", area_index))
                                .debug_selector(move || {
                                    format!("desktop-project-remove-{area_index}")
                                })
                                .label("Remove")
                                .compact()
                                .custom(glass_button_variant(theme, cx))
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.desktop_workspace.actions_area_id = None;
                                    app.begin_desktop_area_remove(remove_id.clone(), cx);
                                })),
                        ),
                );
            }
        }
        if let Some(error) = self.desktop_workspace.area_errors.get(&area_id).cloned() {
            let retry_id = area_id.clone();
            card = card.child(
                div()
                    .id(("desktop-project-unavailable", area_index))
                    .debug_selector(move || format!("desktop-project-unavailable-{area_index}"))
                    .absolute()
                    .left(px(12.0))
                    .right(px(12.0))
                    .bottom(px(10.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgba(theme.destructive))
                            .child(error),
                    )
                    .child(
                        Button::new(("desktop-project-retry", area_index))
                            .debug_selector(move || format!("desktop-project-retry-{area_index}"))
                            .label("Retry")
                            .compact()
                            .custom(glass_button_variant(theme, cx))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                cx.stop_propagation();
                                app.retry_desktop_project_area(retry_id.clone(), window, cx);
                            })),
                    ),
            );
        }
        card
    }
}

pub(super) fn project_identity_label(project: &str, worktree: Option<&str>) -> String {
    match worktree {
        Some(worktree) if worktree != project => format!("{project} / {worktree}"),
        _ => project.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::project_identity_label;

    /// 项目根目录作为 worktree 时不重复显示同一名称，独立 worktree 仍保留完整身份。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   1 个项目名称 + 2 个 worktree 名称；其中 1 个相同，另 1 个不同。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 将相同名称传入标签格式化 → 只显示项目名。
    ///   2. 将不同名称传入标签格式化 → 显示项目与 worktree。
    ///
    /// 预期结果：
    ///   - 正断言：不同 worktree 的完整名称存在。
    ///   - 负断言：相同名称不重复占据卡片标题空间。
    #[test]
    fn project_label_avoids_duplicate_root_worktree() {
        assert_eq!(
            project_identity_label("project-b", Some("feature-nav")),
            "project-b / feature-nav",
            "独立 worktree 应保留完整项目和 worktree 身份"
        );
        assert_eq!(
            project_identity_label("project-b", Some("project-b")),
            "project-b",
            "项目根目录作为 worktree 时不应重复显示项目名"
        );
    }
}
