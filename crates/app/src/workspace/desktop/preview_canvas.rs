use super::{GLASS_TEXT, GLASS_TEXT_MUTED, GLASS_TEXT_SECONDARY};
use crate::app::MultisoulApp;
use crate::cards::chrome::{card_title_background, render_card_title, TITLE_BAR_H};
use crate::cards::{card_for, CardBodyContext};
use canvas::{CanvasNode, CanvasNodeKind, CanvasNodeSnapshot, CanvasRect};
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div};
use storage::DesktopAreaPreview;
use ui::component::StyledExt;
use ui::icons::with_alpha;

pub(super) fn project_stage_rect(size: canvas::Vec2, scale: f32) -> CanvasRect {
    let title = (size.y * 0.14).clamp(20.0 * scale, 44.0 * scale);
    CanvasRect::new(
        10.0 * scale,
        title,
        (size.x - 20.0 * scale).max(1.0),
        (size.y - title - 10.0 * scale).max(1.0),
    )
}

pub(super) fn project_tone(label: &str, theme: ui::Theme) -> u32 {
    let project = label.split(" / ").next().unwrap_or(label);
    let hash = project.bytes().fold(0u32, |value, byte| {
        value.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    [
        theme.accent_violet,
        theme.accent_green,
        theme.accent_blue,
        theme.accent_cyan,
    ][hash as usize % 4]
}

pub(super) fn fitted_rects_at_scale(
    nodes: &[CanvasNodeSnapshot],
    width: f32,
    height: f32,
    scale: f32,
) -> Vec<CanvasRect> {
    let Some(viewport) =
        fitted_viewport(nodes.iter().map(|node| node.bounds), width, height, scale)
    else {
        return Vec::new();
    };
    nodes
        .iter()
        .map(|node| CanvasRect {
            origin: viewport.canvas_to_screen(node.bounds.origin),
            size: canvas::Vec2::new(
                node.bounds.size.x * viewport.zoom,
                node.bounds.size.y * viewport.zoom,
            ),
        })
        .collect()
}

pub(super) fn fitted_viewport(
    bounds: impl IntoIterator<Item = CanvasRect>,
    width: f32,
    height: f32,
    max_zoom: f32,
) -> Option<canvas::Viewport> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let mut bounds = bounds.into_iter();
    let first = bounds.next()?;
    let (mut left, mut top) = (first.origin.x, first.origin.y);
    let (mut right, mut bottom) = (first.max().x, first.max().y);
    for rect in bounds {
        left = left.min(rect.origin.x);
        top = top.min(rect.origin.y);
        right = right.max(rect.max().x);
        bottom = bottom.max(rect.max().y);
    }
    // Keep at least half of a tiny stage for content instead of subtracting a fixed 16px.
    let padding = (8.0 * max_zoom).min(width * 0.25).min(height * 0.25);
    let zoom = ((width - padding * 2.0) / (right - left).max(1.0))
        .min((height - padding * 2.0) / (bottom - top).max(1.0))
        .min(max_zoom);
    Some(canvas::Viewport {
        pan: canvas::Vec2::new(
            (width - (right - left) * zoom) * 0.5 - left * zoom,
            (height - (bottom - top) * zoom) * 0.5 - top * zoom,
        ),
        zoom,
    })
}

fn preview_zoom(bounds: CanvasRect, width: f32, height: f32, readable: bool) -> f32 {
    let width_zoom = width / bounds.size.x.max(1.0);
    let height_zoom = height / bounds.size.y.max(1.0);
    if readable {
        width_zoom.clamp(0.05, 1.0)
    } else {
        width_zoom.min(height_zoom).clamp(0.05, 1.0)
    }
}

impl MultisoulApp {
    pub(super) fn desktop_preview_nodes(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &gpui::App,
    ) -> Vec<CanvasNodeSnapshot> {
        if let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))
        {
            return session.read(cx).runtime.document.to_snapshot().nodes;
        }
        self.desktop_workspace
            .previews
            .get(workspace_id)
            .and_then(|areas| areas.iter().find(|area| area.id == area_id))
            .map(|area| area.nodes.clone())
            .unwrap_or_default()
    }

    pub(super) fn render_desktop_project_preview(
        &self,
        workspace_id: &str,
        area: &DesktopAreaPreview,
        width: f32,
        height: f32,
        scale: f32,
        selector: String,
        cx: &mut Context<Self>,
    ) -> Div {
        let nodes = self.desktop_preview_nodes(workspace_id, &area.id, cx);
        let stage = project_stage_rect(canvas::Vec2::new(width, height), scale);
        let rects = fitted_rects_at_scale(&nodes, stage.size.x, stage.size.y, scale);
        let theme = self.desktop_theme();
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area.id.clone()))
            .map(|entity| entity.read(cx));
        let mut surface = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .rounded(px(14.0 * scale))
            .border_1()
            .border_color(rgba(theme.dock_line))
            .bg(rgba(with_alpha(theme.dock, 0x8c)))
            .overflow_hidden()
            .child(
                div()
                    .id(gpui::ElementId::Name(
                        format!("{selector}-project-label").into(),
                    ))
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .h(px(stage.origin.y))
                    .px(px(12.0 * scale))
                    .flex()
                    .items_center()
                    .font_semibold()
                    .text_size(px(13.0 * scale))
                    .text_color(rgba(GLASS_TEXT))
                    .child(area.name.clone()),
            );
        if nodes.is_empty() {
            return surface
                .debug_selector(move || format!("{selector}-empty"))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .text_size(px(12.0 * scale))
                .child(
                    div()
                        .text_color(rgba(GLASS_TEXT_MUTED))
                        .child("No cards yet"),
                );
        }
        for (node_index, (node, rect)) in nodes.into_iter().zip(rects).enumerate() {
            let preview_selector = format!("{selector}-node-{node_index}");
            let card = self.render_desktop_preview_card(
                &node,
                session.as_deref(),
                rect.size.x,
                rect.size.y,
                false,
                preview_selector,
                None,
            );
            let positioned = div()
                .id(gpui::ElementId::Name(
                    format!("{selector}-hit-{node_index}").into(),
                ))
                .absolute()
                .left(px(stage.origin.x + rect.origin.x))
                .top(px(stage.origin.y + rect.origin.y))
                .w(px(rect.size.x))
                .h(px(rect.size.y))
                .child(card);
            surface = surface.child(positioned);
        }
        surface
    }

    pub(super) fn render_desktop_preview_card(
        &self,
        snapshot: &CanvasNodeSnapshot,
        session: Option<&MultisoulApp>,
        width: f32,
        height: f32,
        readable: bool,
        selector: String,
        project_label: Option<String>,
    ) -> AnyElement {
        let theme = self.desktop_theme();
        let node = CanvasNode {
            id: snapshot.id,
            kind: snapshot.kind.clone(),
            bounds: snapshot.bounds,
            z_index: snapshot.z_index,
            selected: false,
        };
        let zoom = preview_zoom(snapshot.bounds, width, height, readable);
        let chrome = session.map(|app| card_for(node.kind.node_type()).chrome(&node, &app.runtime));
        let title = chrome
            .as_ref()
            .map(|chrome| chrome.title.clone())
            .unwrap_or_else(|| node.kind.display_title());
        let has_project_label = project_label.is_some();
        let detail = if has_project_label {
            None
        } else {
            chrome.and_then(|chrome| chrome.title_detail)
        };
        let project_color = project_label
            .as_deref()
            .map(|label| project_tone(label, theme))
            .unwrap_or(theme.accent_blue);
        let title_height = if has_project_label && readable {
            54.0
        } else if has_project_label {
            16.0_f32.min(height)
        } else {
            (TITLE_BAR_H * zoom).max(12.0).min(height)
        };
        let kind = match &node.kind {
            CanvasNodeKind::Agent(_) => "AGENT",
            CanvasNodeKind::Term(_) => "TERMINAL",
            CanvasNodeKind::Browser(_) => "BROWSER",
            CanvasNodeKind::Preview(_) => "PREVIEW",
            CanvasNodeKind::Rect(_) => "NODE",
        };
        let body = session.map(|app| {
            card_for(node.kind.node_type()).body(
                &node,
                CardBodyContext {
                    runtime: &app.runtime,
                    terminal_render_cache: app.terminal_render_cache.clone(),
                    preview_state: app.previews.state_for(node.id),
                    preview_image_viewer: app.previews.image_viewer_presentation(node.id),
                    preview_image_dragging: false,
                    text_selection: None,
                    focused: false,
                    reduce_motion: true,
                    initialization: None,
                    terminal_cursor_phase: app.terminal_cursor_phase,
                    browser_address_input: None,
                    browser_auth_state: app.browser_chrome_auth_state_for(node.id),
                    ego_preview: app.ego_previews.frame_for(node.id),
                    #[cfg(all(target_os = "macos", not(feature = "e2e-test-support")))]
                    ego_live_preview: None,
                    ego_preview_error: app.ego_previews.error_for(node.id),
                    occluded_by_overlay: matches!(node.kind, CanvasNodeKind::Browser(_)),
                    theme,
                    zoom,
                    bottom_radius_px: 5.0,
                },
            )
        });
        div()
            .debug_selector(move || selector.clone())
            .w(px(width))
            .h(px(height))
            .flex()
            .flex_col()
            .rounded(px(5.0))
            .border_1()
            .border_color(rgba(theme.node_stroke))
            .bg(rgba(theme.node_fill))
            .overflow_hidden()
            .child(
                div()
                    .h(px(title_height))
                    .w_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .px(px((8.0 * zoom).max(3.0)))
                    .when(readable, |this| this.pr(px(50.0)))
                    .bg(card_title_background(theme.title_bar, theme))
                    .text_size(px((12.0 * zoom).max(7.0)))
                    .text_color(rgba(GLASS_TEXT))
                    .when_some(project_label, |this, project| {
                        if !readable {
                            return this.child(
                                div()
                                    .max_w(px(width - 8.0))
                                    .truncate()
                                    .px(px(3.0))
                                    .rounded(px(4.0))
                                    .bg(rgba(with_alpha(project_color, 0x2e)))
                                    .text_size(px(9.0))
                                    .text_color(rgba(project_color))
                                    .child(project),
                            );
                        }
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(7.0))
                                .text_size(px(10.0))
                                .text_color(rgba(GLASS_TEXT_MUTED))
                                .child(div().w(px(7.0)).h(px(7.0)).rounded(px(4.0)).bg(rgba(
                                    if session.is_some() {
                                        theme.success
                                    } else {
                                        GLASS_TEXT_MUTED
                                    },
                                )))
                                .child(kind)
                                .child(
                                    div()
                                        .max_w(px((width - 160.0).max(80.0)))
                                        .truncate()
                                        .px(px(6.0))
                                        .py(px(2.0))
                                        .rounded(px(6.0))
                                        .bg(rgba(with_alpha(project_color, 0x2e)))
                                        .text_color(rgba(project_color))
                                        .child(project),
                                ),
                        )
                    })
                    .when(readable || !has_project_label, |this| {
                        this.child(render_card_title(node.id, title, detail, zoom, theme))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .bg(rgba(theme.term_bg))
                    .text_color(rgba(GLASS_TEXT_SECONDARY))
                    .child(
                        body.unwrap_or_else(|| div().bg(rgba(theme.term_bg)).into_any_element()),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 项目身份色只取项目名：同一项目的不同 worktree 使用同一种颜色。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   alpha 的两个标签共享项目名前缀；beta 是另一项目；调色板含 4 种颜色。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 分别计算 alpha / main、alpha / feature 与 beta / main 的颜色。
    ///   2. 比较同项目和不同项目的结果。
    ///
    /// 预期结果：
    ///   - 正断言：alpha 的两张卡片颜色相同。
    ///   - 负断言：beta 不使用 alpha 的颜色，避免相邻项目难以区分。
    #[test]
    fn project_tone_uses_project_identity_across_worktrees() {
        let theme = ui::Theme::sapphire_night();
        let alpha = project_tone("alpha / main", theme);
        assert_eq!(
            alpha,
            project_tone("alpha / feature", theme),
            "同一项目的不同 worktree 必须保留一致的身份色"
        );
        assert_ne!(
            alpha,
            project_tone("beta / main", theme),
            "不同项目应使用不同身份色，以便跨项目总览辨认"
        );
    }

    /// 展开 Agent 时按原卡片宽度保持文字可读，紧凑缩略图仍按两轴等比缩小。
    ///
    /// 数据构造：原卡片 800×600；缩略区域 400×300，两轴比例均为 0.5；展开区域 800×300，宽度比例 1、高度比例 0.5。
    ///
    /// 执行过程：分别计算紧凑缩略和展开态 zoom，并比较实际值。
    ///
    /// 预期结果：缩略 zoom = 0.5；展开 zoom = 1.0；展开态不得因高度限制降为 0.5。
    #[test]
    fn expanded_agent_keeps_readable_text_scale() {
        let bounds = CanvasRect::new(0.0, 0.0, 800.0, 600.0);
        assert_eq!(
            preview_zoom(bounds, 400.0, 300.0, false),
            0.5,
            "紧凑缩略图应按宽高等比缩小"
        );
        assert_eq!(
            preview_zoom(bounds, 800.0, 300.0, true),
            1.0,
            "展开态应保持与原画布相同的可读字号"
        );
        assert_ne!(
            preview_zoom(bounds, 800.0, 300.0, true),
            0.5,
            "展开态不得沿用高度限制导致文字缩半"
        );
    }

    /// 真实画布缩略图按节点原始边界等比缩小并保留相对位置。
    ///
    /// 数据构造：第一张 400×300 位于 (0,0)，第二张 400×300 位于 (500,100)；总宽 900，高 400。
    /// 目标 450×200，扣除两侧各 8px 后缩放系数 min(434/900,184/400) = 0.46。
    ///
    /// 执行过程：1. 将原始节点传入 fitted_rects；2. 计算两张卡片缩略边界。
    ///
    /// 预期结果：第一张在左，第二张在右且更低；两张尺寸相同，不按列表顺序重排。
    #[test]
    fn thumbnail_keeps_canvas_geometry() {
        let nodes = [
            CanvasNodeSnapshot {
                id: 1,
                kind: CanvasNodeKind::Term(canvas::TermSpec),
                bounds: CanvasRect::new(0.0, 0.0, 400.0, 300.0),
                z_index: 1,
            },
            CanvasNodeSnapshot {
                id: 2,
                kind: CanvasNodeKind::Term(canvas::TermSpec),
                bounds: CanvasRect::new(500.0, 100.0, 400.0, 300.0),
                z_index: 2,
            },
        ];
        let fitted = fitted_rects_at_scale(&nodes, 450.0, 200.0, 1.0);
        assert_eq!(fitted.len(), 2, "两张真实卡片都必须出现在缩略图中");
        assert!(
            fitted[0].origin.x < fitted[1].origin.x,
            "第二张卡片必须保留在第一张右侧"
        );
        assert!(
            fitted[0].origin.y < fitted[1].origin.y,
            "第二张卡片必须保留在第一张下侧"
        );
        assert_eq!(
            fitted[0].size, fitted[1].size,
            "相同原始尺寸必须得到相同缩略尺寸"
        );
        assert_ne!(
            fitted[0].origin, fitted[1].origin,
            "缩略图不得退化为堆叠列表"
        );
    }
}
