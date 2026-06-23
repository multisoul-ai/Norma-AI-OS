//! Terminal card renderer.

use crate::cards::chrome::{CardChrome, TITLE_BAR_H};
use crate::cards::term::view::{render_terminal_node, TERM_CELL_PX, TERM_LINE_PX, TERM_PADDING_PX};
use crate::cards::{Card, CardBodyContext};
use canvas::{CanvasNode, CanvasRect, NodeType};
use gpui::AnyElement;
use runtime::{NodeResize, Runtime};

const MIN_TERMINAL_COLS: u16 = 20;
const MIN_TERMINAL_ROWS: u16 = 3;

pub struct TermCard;

pub static CARD: TermCard = TermCard;

impl Card for TermCard {
    fn ty(&self) -> NodeType {
        NodeType::Term
    }

    fn chrome(&self, _node: &CanvasNode, _runtime: &Runtime) -> CardChrome {
        CardChrome {
            title: "Terminal".to_string(),
            refreshable: false,
            task_badge: None,
        }
    }

    fn body(&self, node: &CanvasNode, context: CardBodyContext<'_>) -> AnyElement {
        let selection = context
            .text_selection
            .filter(|selection| selection.node_id == node.id && !selection.is_empty())
            .map(|selection| selection.range());
        render_terminal_node(
            context.runtime.node_view(node.id),
            context.theme,
            context.zoom,
            context.bottom_radius_px,
            selection,
        )
    }

    fn resize(&self, bounds: CanvasRect, zoom: f32) -> Option<NodeResize> {
        Some(term_resize(bounds, zoom))
    }

    fn wheel_scroll_mode(&self) -> crate::cards::CardWheelScrollMode {
        crate::cards::CardWheelScrollMode::TerminalScrollback
    }
}

pub(crate) fn term_resize(bounds: CanvasRect, zoom: f32) -> NodeResize {
    let zoom = zoom.max(0.001);
    let screen_w = (bounds.size.x * zoom).max(0.0);
    let screen_h = (bounds.size.y * zoom).max(0.0);
    let content_w = (screen_w - TERM_PADDING_PX * 2.0).max(0.0);
    let content_h = (screen_h - TITLE_BAR_H * zoom - TERM_PADDING_PX * 2.0).max(0.0);
    let cols = clamp_terminal_dimension(content_w / (TERM_CELL_PX * zoom), MIN_TERMINAL_COLS);
    let rows = clamp_terminal_dimension(content_h / (TERM_LINE_PX * zoom), MIN_TERMINAL_ROWS);
    NodeResize::TermGrid { cols, rows }
}

fn clamp_terminal_dimension(raw: f32, min: u16) -> u16 {
    raw.floor().clamp(f32::from(min), f32::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TermCard resize 将卡片 bounds 换算为 terminal grid 尺寸。
    ///
    /// 数据构造（含关键数值推导）：
    ///   bounds = size(800x426)，zoom=1
    ///   padding = 8px * 2 = 16px
    ///   title bar = 26px
    ///   content_w = 800 - 16 = 784；TERM_CELL_PX=8 → cols=floor(784/8)=98
    ///   content_h = 426 - 26 - 16 = 384；TERM_LINE_PX=16 → rows=floor(384/16)=24
    ///
    /// 执行过程：
    ///   1. term_resize(bounds, 1.0)
    ///   2. 拆出 NodeResize::TermGrid
    ///
    /// 预期结果：
    ///   - 正断言：cols=98, rows=24
    ///   - 负断言：cols 不应退回最小值 20，rows 不应退回最小值 3
    #[test]
    fn test_term_resize_converts_card_bounds_to_grid_dimensions() {
        let resize = term_resize(CanvasRect::new(0.0, 0.0, 800.0, 426.0), 1.0);
        let NodeResize::TermGrid { cols, rows } = resize;

        assert_eq!(cols, 98, "800px 宽卡片应换算为 98 列");
        assert_eq!(rows, 24, "426px 高卡片应换算为 24 行");
        assert_ne!(cols, 20, "宽度充足时 cols 不应退回最小值 20");
        assert_ne!(rows, 3, "高度充足时 rows 不应退回最小值 3");
    }

    /// TermCard chrome 不显示 Agent task badge。
    ///
    /// 数据构造（含关键数值推导）：
    ///   Terminal node: id=1, bounds=100x80
    ///   runtime 为空；Terminal card 不读取 Agent task state
    ///
    /// 执行过程：
    ///   1. 构造 Terminal CanvasNode
    ///   2. 调用 TermCard.chrome
    ///
    /// 预期结果：
    ///   - 正断言：title 为 Terminal
    ///   - 负断言：task_badge 必须为 None
    #[test]
    fn test_term_card_chrome_has_no_task_badge() {
        let runtime = Runtime::new();
        let node = CanvasNode {
            id: 1,
            kind: canvas::CanvasNodeKind::Term(canvas::TermSpec),
            bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
            z_index: 1,
            selected: false,
        };

        let chrome = CARD.chrome(&node, &runtime);

        assert_eq!(
            chrome.title, "Terminal",
            "Terminal card 标题应保持 Terminal"
        );
        assert!(
            chrome.task_badge.is_none(),
            "Terminal card 不应显示 Agent task badge"
        );
    }
}
