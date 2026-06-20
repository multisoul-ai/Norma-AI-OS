//! 文档：节点集合 + viewport（pan/zoom 状态）。

use crate::geometry::{CanvasRect, Vec2, Viewport};
use crate::node::{CanvasNode, CanvasNodeKind, NodeId};

#[derive(Debug)]
pub struct Document {
    nodes: Vec<CanvasNode>,
    pub viewport: Viewport,
    next_id: NodeId,
    max_z: i32,
}

impl Default for Document {
    fn default() -> Self {
        Document::new()
    }
}

impl Document {
    pub fn new() -> Self {
        Document { nodes: Vec::new(), viewport: Viewport::default(), next_id: 1, max_z: 0 }
    }

    /// 添加节点：分配递增 id，z_index 抬到当前最高层 +1。
    pub fn add_node(&mut self, kind: CanvasNodeKind, bounds: CanvasRect) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.max_z += 1;
        self.nodes.push(CanvasNode { id, kind, bounds, z_index: self.max_z, selected: false });
        id
    }

    pub fn nodes(&self) -> &[CanvasNode] {
        &self.nodes
    }

    pub fn node(&self, id: NodeId) -> Option<&CanvasNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut CanvasNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// 在画布坐标系下平移节点。delta 已是画布空间向量（调用方负责屏幕→画布换算）。
    pub fn move_node(&mut self, id: NodeId, delta: Vec2) {
        if let Some(n) = self.node_mut(id) {
            n.bounds.origin = n.bounds.origin + delta;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{CanvasRect, Vec2};
    use crate::node::{CanvasNodeKind, RectNode};

    /// add_node 分配递增 id 且 z_index 逐个抬升（后加的在上层）。
    ///
    /// 数据构造：
    ///   连续 add 两个 Rect 节点 A、B
    ///
    /// 执行过程：
    ///   1. add A → id=1, z_index=1（首个，基线 z=1）
    ///   2. add B → id=2, z_index=2（在 A 之上）
    ///
    /// 预期结果：
    ///   - 正断言：A.id=1, B.id=2（id 递增、唯一）
    ///   - 正断言：B.z_index > A.z_index（新节点在上层）
    ///   - 负断言：A.id != B.id（不得重复分配）
    #[test]
    fn test_add_node_assigns_incrementing_id_and_zindex() {
        let mut doc = Document::new();
        let a = doc.add_node(CanvasNodeKind::Rect(RectNode { color: 0x4080ff }), CanvasRect::new(0.0, 0.0, 100.0, 80.0));
        let b = doc.add_node(CanvasNodeKind::Rect(RectNode { color: 0xff8040 }), CanvasRect::new(50.0, 50.0, 100.0, 80.0));

        assert_eq!(a, 1, "首个节点 id 应为 1");
        assert_eq!(b, 2, "第二个节点 id 应为 2（递增）");
        assert_ne!(a, b, "两个节点 id 不得相同");

        let za = doc.node(a).unwrap().z_index;
        let zb = doc.node(b).unwrap().z_index;
        assert!(zb > za, "后添加的 B 应在更高 z_index（{} > {}）", zb, za);
    }

    /// move_node 在画布坐标系下平移节点 bounds.origin，size 不变。
    ///
    /// 数据构造：
    ///   节点 bounds = origin(10,10) size(100x80)
    ///   delta = (15, -5)
    ///
    /// 执行过程：
    ///   move_node(id, (15,-5)) → origin = (10+15, 10-5) = (25, 5)
    ///
    /// 预期结果：
    ///   - 正断言：origin == (25, 5)
    ///   - 正断言：size 仍为 (100, 80)（移动不改尺寸）
    ///   - 负断言：origin != (10, 10)（确实移动了）
    #[test]
    fn test_move_node_translates_origin_keeps_size() {
        let mut doc = Document::new();
        let id = doc.add_node(CanvasNodeKind::Rect(RectNode { color: 0x4080ff }), CanvasRect::new(10.0, 10.0, 100.0, 80.0));

        doc.move_node(id, Vec2::new(15.0, -5.0));

        let n = doc.node(id).unwrap();
        assert_eq!(n.bounds.origin, Vec2::new(25.0, 5.0), "origin 应平移到 (25,5)");
        assert_eq!(n.bounds.size, Vec2::new(100.0, 80.0), "size 不应被 move 改变");
        assert_ne!(n.bounds.origin, Vec2::new(10.0, 10.0), "origin 应已改变，证明 move 生效");
    }

    /// 不存在的 id 查询返回 None（负路径）。
    #[test]
    fn test_node_lookup_missing_returns_none() {
        let doc = Document::new();
        assert!(doc.node(999).is_none(), "未创建的 id=999 应返回 None");
    }
}
