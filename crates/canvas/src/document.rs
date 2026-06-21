//! 文档：节点集合 + viewport（pan/zoom 状态）。

use crate::geometry::{CanvasRect, Vec2, Viewport};
use crate::node::{CanvasNode, CanvasNodeKind, NodeId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CanvasNodeSnapshot {
    pub id: NodeId,
    pub kind: CanvasNodeKind,
    pub bounds: CanvasRect,
    pub z_index: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DocumentSnapshot {
    pub nodes: Vec<CanvasNodeSnapshot>,
    pub viewport: Viewport,
}

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
        Document {
            nodes: Vec::new(),
            viewport: Viewport::default(),
            next_id: 1,
            max_z: 0,
        }
    }

    pub fn to_snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            nodes: self
                .nodes
                .iter()
                .map(|node| CanvasNodeSnapshot {
                    id: node.id,
                    kind: node.kind,
                    bounds: node.bounds,
                    z_index: node.z_index,
                })
                .collect(),
            viewport: self.viewport,
        }
    }

    pub fn from_snapshot(snapshot: DocumentSnapshot) -> Self {
        let mut next_id = 1;
        let mut max_z = 0;
        let nodes = snapshot
            .nodes
            .into_iter()
            .map(|node| {
                next_id = next_id.max(node.id.saturating_add(1));
                max_z = max_z.max(node.z_index);
                CanvasNode {
                    id: node.id,
                    kind: node.kind,
                    bounds: node.bounds,
                    z_index: node.z_index,
                    selected: false,
                }
            })
            .collect();

        Document {
            nodes,
            viewport: snapshot.viewport,
            next_id,
            max_z,
        }
    }

    /// 添加节点：分配递增 id，z_index 抬到当前最高层 +1。
    pub fn add_node(&mut self, kind: CanvasNodeKind, bounds: CanvasRect) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.max_z += 1;
        self.nodes.push(CanvasNode {
            id,
            kind,
            bounds,
            z_index: self.max_z,
            selected: false,
        });
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

    /// 直接设置节点 bounds；不存在的 id 忽略。
    pub fn set_node_bounds(&mut self, id: NodeId, bounds: CanvasRect) {
        if let Some(n) = self.node_mut(id) {
            n.bounds = bounds;
        }
    }

    /// 移除指定节点并返回被移除的节点；不存在的 id 返回 None。
    pub fn remove_node(&mut self, id: NodeId) -> Option<CanvasNode> {
        let index = self.nodes.iter().position(|node| node.id == id)?;
        Some(self.nodes.remove(index))
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
        let a = doc.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
            CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        );
        let b = doc.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0xff8040 }),
            CanvasRect::new(50.0, 50.0, 100.0, 80.0),
        );

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
        let id = doc.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
            CanvasRect::new(10.0, 10.0, 100.0, 80.0),
        );

        doc.move_node(id, Vec2::new(15.0, -5.0));

        let n = doc.node(id).unwrap();
        assert_eq!(
            n.bounds.origin,
            Vec2::new(25.0, 5.0),
            "origin 应平移到 (25,5)"
        );
        assert_eq!(
            n.bounds.size,
            Vec2::new(100.0, 80.0),
            "size 不应被 move 改变"
        );
        assert_ne!(
            n.bounds.origin,
            Vec2::new(10.0, 10.0),
            "origin 应已改变，证明 move 生效"
        );
    }

    /// set_node_bounds 只更新指定节点 bounds，不影响其它节点。
    ///
    /// 数据构造（含关键数值推导）：
    ///   A 初始 bounds = origin(0,0) size(100x80)
    ///   B 初始 bounds = origin(200,0) size(100x80)
    ///   A 新 bounds = origin(10,20) size(300x200)
    ///
    /// 执行过程：
    ///   1. add A → id=1
    ///   2. add B → id=2
    ///   3. set_node_bounds(A, new_bounds) → 仅改 A.bounds
    ///   4. 读取 A/B 验证状态
    ///
    /// 预期结果：
    ///   - 正断言：A bounds 更新为 new_bounds
    ///   - 正断言：B bounds 仍为原始值
    ///   - 负断言：B bounds 不应等于 A 的 new_bounds
    #[test]
    fn test_set_node_bounds_updates_target_and_leaves_other_node_unchanged() {
        let mut doc = Document::new();
        let a = doc.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
            CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        );
        let b = doc.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0xff8040 }),
            CanvasRect::new(200.0, 0.0, 100.0, 80.0),
        );
        let new_bounds = CanvasRect::new(10.0, 20.0, 300.0, 200.0);

        doc.set_node_bounds(a, new_bounds);

        let node_a = doc.node(a).unwrap();
        let node_b = doc.node(b).unwrap();
        assert_eq!(node_a.bounds, new_bounds, "A bounds 应更新为 new_bounds");
        assert_eq!(
            node_b.bounds,
            CanvasRect::new(200.0, 0.0, 100.0, 80.0),
            "B bounds 应保持原始值，set_node_bounds 不应影响其它节点"
        );
        assert_ne!(
            node_b.bounds, new_bounds,
            "B bounds 不应等于 A 的 new_bounds，否则说明错误更新了多个节点"
        );
    }

    /// from_snapshot 恢复节点、viewport，并让后续新增节点 id 接在历史最大 id 后面。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   snapshot.nodes = [id=4,z=7,bounds(10,20,300,200)]
    ///   viewport.pan = (12,34)，viewport.zoom = 1.5
    ///   恢复后 next_id 应为 max_id(4)+1 = 5
    ///   恢复后 max_z 应为 max_z(7)，新增节点 z_index 应为 8
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. Document::from_snapshot(snapshot) → 构造 Document
    ///   2. 查询 id=4 节点与 viewport，确认状态来自快照
    ///   3. add_node(Rect) → 新节点应拿到 id=5 且 z_index=8
    ///
    /// 预期结果：
    ///   - 正断言：恢复节点 id=4、bounds、viewport 均保留
    ///   - 正断言：新增节点 id=5，证明 next_id 按 max_id+1 推导
    ///   - 负断言：新增节点不应复用 id=4，避免覆盖历史节点
    #[test]
    fn test_from_snapshot_restores_document_and_allocates_next_id_after_max() {
        let snapshot = DocumentSnapshot {
            nodes: vec![CanvasNodeSnapshot {
                id: 4,
                kind: CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
                bounds: CanvasRect::new(10.0, 20.0, 300.0, 200.0),
                z_index: 7,
            }],
            viewport: Viewport {
                pan: Vec2::new(12.0, 34.0),
                zoom: 1.5,
            },
        };

        let mut doc = Document::from_snapshot(snapshot);
        let restored_bounds = doc.node(4).expect("snapshot id=4 节点应存在").bounds;
        let new_id = doc.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0xff8040 }),
            CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        );

        assert_eq!(
            restored_bounds,
            CanvasRect::new(10.0, 20.0, 300.0, 200.0),
            "from_snapshot 应恢复节点 bounds"
        );
        assert_eq!(doc.viewport.zoom, 1.5, "from_snapshot 应恢复 viewport.zoom");
        assert_eq!(new_id, 5, "恢复 max_id=4 后新增节点 id 应为 5");
        assert_ne!(
            new_id, 4,
            "新增节点不应复用已恢复的 id=4，否则会破坏持久化节点身份"
        );
        assert_eq!(
            doc.node(new_id).expect("新增节点应存在").z_index,
            8,
            "恢复 max_z=7 后新增节点 z_index 应为 8"
        );
    }

    /// remove_node 移除指定节点并返回被移除的节点，其它节点保持原顺序与 bounds。
    ///
    /// 数据构造（含关键数值推导）：
    ///   A: id=1, color=0x4080ff, bounds=(0,0,100,80)
    ///   B: id=2, color=0xff8040, bounds=(200,0,120,90)
    ///   C: id=3, color=0x35d07f, bounds=(400,0,140,100)
    ///   add_node 从 1 递增，因此删除 B 后应只剩 id=1 与 id=3
    ///
    /// 执行过程：
    ///   1. add A/B/C → document.nodes() = [1,2,3]
    ///   2. remove_node(B) → 从 Vec 中移除 id=2，并返回原 B 节点
    ///   3. 查询 A/B/C 与节点列表验证状态
    ///   4. remove_node(999) → 不存在的 id 不改变文档
    ///
    /// 预期结果：
    ///   - 正断言：返回的 removed.id == B，且 B 已无法查询
    ///   - 正断言：A 与 C 仍存在，C bounds 保持原值
    ///   - 负断言：nodes 中不再包含 B，remove_node(999) 不应返回节点
    #[test]
    fn test_remove_node_deletes_target_and_preserves_others() {
        let mut doc = Document::new();
        let a = doc.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
            CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        );
        let b = doc.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0xff8040 }),
            CanvasRect::new(200.0, 0.0, 120.0, 90.0),
        );
        let c_bounds = CanvasRect::new(400.0, 0.0, 140.0, 100.0);
        let c = doc.add_node(CanvasNodeKind::Rect(RectNode { color: 0x35d07f }), c_bounds);

        let removed = doc.remove_node(b);

        assert!(
            removed.is_some(),
            "remove_node(B) 应返回 Some，被删除节点需要交给 runtime 做资源清理"
        );
        assert_eq!(
            removed.unwrap().id,
            b,
            "remove_node(B) 返回的节点 id 应等于被请求删除的 B"
        );
        assert!(doc.node(a).is_some(), "删除 B 不应影响 A，A 应仍可被查询到");
        assert!(
            doc.node(b).is_none(),
            "删除 B 后 node(B) 应返回 None，证明卡片已从文档移除"
        );
        assert_eq!(
            doc.node(c).expect("C 应仍存在").bounds,
            c_bounds,
            "删除 B 不应改动 C 的 bounds"
        );
        assert!(
            !doc.nodes().iter().any(|node| node.id == b),
            "nodes 列表不应再包含 B，否则渲染层仍会显示这张卡片"
        );

        let missing = doc.remove_node(999);
        assert!(
            missing.is_none(),
            "remove_node(999) 应返回 None，不存在的节点不能伪造删除结果"
        );
        assert_eq!(
            doc.nodes().len(),
            2,
            "删除不存在节点后节点数应保持 2，不应误删 A 或 C"
        );
    }

    /// 不存在的 id 查询返回 None（负路径）。
    #[test]
    fn test_node_lookup_missing_returns_none() {
        let doc = Document::new();
        assert!(doc.node(999).is_none(), "未创建的 id=999 应返回 None");
    }
}
