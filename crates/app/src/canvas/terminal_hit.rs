use crate::app::MultisoulApp;
use crate::canvas::geometry::terminal_content_local_position;
use crate::cards::chrome::TITLE_BAR_H;
use crate::cards::term::selection::terminal_point_for_local_position;
use crate::cards::term::view::{terminal_render_metrics, TERM_PADDING_PX};
use canvas::{NodeId, Vec2};
use runtime::NodeView;

use super::text_selection::TextPoint;

pub(crate) fn terminal_text_point_for_screen(
    app: &MultisoulApp,
    node_id: NodeId,
    screen: Vec2,
) -> Option<TextPoint> {
    let node = app.runtime.document.node(node_id)?;
    let viewport = app.runtime.document.viewport;
    let node_top_left = viewport.canvas_to_screen(node.bounds.origin);
    let NodeView::Term { snapshot, .. } = app.runtime.node_view(node_id)?;
    let local = terminal_content_local_position(screen, node_top_left, viewport.zoom);
    Some(terminal_point_for_local_position(
        local,
        &snapshot,
        terminal_render_metrics(viewport.zoom),
    ))
}

/// 选中专用：返回 scrollback 绝对行坐标的 TextPoint。
///
/// 绝对行 0 = 最旧 scrollback 行；绝对行 `history_size` = 可见屏幕顶行
/// （display_offset = 0 时）。与 [`terminal_text_point_for_screen`] 不同，后者
/// 返回视口相对行号，供滚轮 SGR 上报使用；选中必须用绝对坐标才能在滚动回退时
/// 保持选区稳定。
pub(crate) fn terminal_selection_point_for_screen(
    app: &MultisoulApp,
    node_id: NodeId,
    screen: Vec2,
) -> Option<TextPoint> {
    let node = app.runtime.document.node(node_id)?;
    let viewport = app.runtime.document.viewport;
    let node_top_left = viewport.canvas_to_screen(node.bounds.origin);
    let NodeView::Term { snapshot, .. } = app.runtime.node_view(node_id)?;
    let local = terminal_content_local_position(screen, node_top_left, viewport.zoom);
    let viewport_point =
        terminal_point_for_local_position(local, &snapshot, terminal_render_metrics(viewport.zoom));
    // 绝对行 = 视口行 + history_size - display_offset
    let absolute_row = viewport_point.row + snapshot.history_size - snapshot.display_offset;
    Some(TextPoint::new(absolute_row, viewport_point.col))
}

/// 返回终端内容区域的屏幕坐标矩形 (top_left, size)。
/// 用于判断鼠标是否在内容区域外，触发自动滚动。
///
/// 与 `terminal_content_local_position` 的逆映射保持一致：padding 不随 zoom
/// 缩放，标题栏高度随 zoom 缩放。
pub(crate) fn terminal_content_rect(app: &MultisoulApp, node_id: NodeId) -> Option<(Vec2, Vec2)> {
    let node = app.runtime.document.node(node_id)?;
    let viewport = app.runtime.document.viewport;
    let node_top_left = viewport.canvas_to_screen(node.bounds.origin);
    let NodeView::Term { snapshot, .. } = app.runtime.node_view(node_id)?;
    let metrics = terminal_render_metrics(viewport.zoom);
    let width = f32::from(metrics.cell_width) * snapshot.cols as f32;
    let height = f32::from(metrics.line_height) * snapshot.lines as f32;
    let top_left = Vec2::new(
        node_top_left.x + TERM_PADDING_PX,
        node_top_left.y + TITLE_BAR_H * viewport.zoom + TERM_PADDING_PX,
    );
    Some((top_left, Vec2::new(width, height)))
}

/// 链接命中专用坐标换算：只接受真实终端网格内容区域，不能把 padding 钳制为边缘 cell。
pub(crate) fn terminal_link_point_for_screen(
    app: &MultisoulApp,
    node_id: NodeId,
    screen: Vec2,
) -> Option<TextPoint> {
    let node = app.runtime.document.node(node_id)?;
    let viewport = app.runtime.document.viewport;
    let node_top_left = viewport.canvas_to_screen(node.bounds.origin);
    let NodeView::Term { snapshot, .. } = app.runtime.node_view(node_id)?;
    let local = terminal_content_local_position(screen, node_top_left, viewport.zoom);
    let metrics = terminal_render_metrics(viewport.zoom);
    let x = f32::from(local.x);
    let y = f32::from(local.y);
    let width = f32::from(metrics.cell_width) * snapshot.cols as f32;
    let height = f32::from(metrics.line_height) * snapshot.lines as f32;
    if x < 0.0 || y < 0.0 || x >= width || y >= height {
        return None;
    }
    Some(terminal_point_for_local_position(local, &snapshot, metrics))
}
