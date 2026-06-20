//! 命中测试（零 gpui）。返回命中且 z_index 最高的节点。

use crate::geometry::Vec2;
use crate::node::{CanvasNode, NodeId};

/// 在画布坐标系下对 point 做命中测试。
/// 遍历所有节点，取 bounds.contains(point) 且 z_index 最大者；无命中返回 None。
pub fn hit_test(nodes: &[CanvasNode], point: Vec2) -> Option<NodeId> {
    nodes
        .iter()
        .filter(|n| n.bounds.contains(point))
        .max_by_key(|n| n.z_index)
        .map(|n| n.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::geometry::{CanvasRect, Vec2};
    use crate::node::{CanvasNodeKind, RectNode};

    /// 命中重叠区域时返回 z_index 最高（最上层）的节点。
    ///
    /// 数据构造：
    ///   A: origin(0,0)   size(100x100) → 覆盖 [0,100)x[0,100)，先加 → z=1
    ///   B: origin(50,50) size(100x100) → 覆盖 [50,150)x[50,150)，后加 → z=2（更高）
    ///   重叠区：x∈[50,100), y∈[50,100)
    ///   命中点 P = (60,60) → 同时落在 A 和 B 内
    ///
    /// 执行过程：
    ///   hit_test 遍历 → A、B 均 contains(P) → 选 z_index 较大者 = B
    ///
    /// 预期结果：
    ///   - 正断言：返回 B（最上层）
    ///   - 负断言：不返回 A（被 B 遮挡，即使也命中）
    #[test]
    fn test_hit_returns_topmost_in_overlap() {
        let mut doc = Document::new();
        let a = doc.add_node(CanvasNodeKind::Rect(RectNode { color: 0x111111 }), CanvasRect::new(0.0, 0.0, 100.0, 100.0));
        let b = doc.add_node(CanvasNodeKind::Rect(RectNode { color: 0x222222 }), CanvasRect::new(50.0, 50.0, 100.0, 100.0));

        let hit = hit_test(doc.nodes(), Vec2::new(60.0, 60.0));
        assert_eq!(hit, Some(b), "重叠区应命中最上层 B（z 更高）");
        assert_ne!(hit, Some(a), "不应命中被遮挡的 A");
    }

    /// 点不在任何节点内时返回 None。
    ///
    /// 数据构造：
    ///   单节点 origin(0,0) size(10x10) → 覆盖 [0,10)x[0,10)
    ///   命中点 (500,500) 明显在外
    ///
    /// 预期结果：
    ///   - 负断言：返回 None（无命中）
    #[test]
    fn test_miss_returns_none() {
        let mut doc = Document::new();
        doc.add_node(CanvasNodeKind::Rect(RectNode { color: 0x111111 }), CanvasRect::new(0.0, 0.0, 10.0, 10.0));

        let hit = hit_test(doc.nodes(), Vec2::new(500.0, 500.0));
        assert_eq!(hit, None, "点在所有节点之外，应返回 None");
    }

    /// 仅命中唯一覆盖该点的节点（非重叠区域，正路径）。
    ///
    /// 数据构造：同上两节点 A(z1)/B(z2)，命中点 (10,10) 仅落在 A 内（不在 B 的 [50,150) 区间）
    ///
    /// 预期结果：
    ///   - 正断言：返回 A
    ///   - 负断言：不返回 B（B 未覆盖该点）
    #[test]
    fn test_hit_only_covering_node() {
        let mut doc = Document::new();
        let a = doc.add_node(CanvasNodeKind::Rect(RectNode { color: 0x111111 }), CanvasRect::new(0.0, 0.0, 100.0, 100.0));
        let b = doc.add_node(CanvasNodeKind::Rect(RectNode { color: 0x222222 }), CanvasRect::new(50.0, 50.0, 100.0, 100.0));

        let hit = hit_test(doc.nodes(), Vec2::new(10.0, 10.0));
        assert_eq!(hit, Some(a), "(10,10) 仅落在 A 内，应命中 A");
        assert_ne!(hit, Some(b), "B 未覆盖 (10,10)，不应命中 B");
    }
}
