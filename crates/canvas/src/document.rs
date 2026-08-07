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
                    kind: node.kind.clone(),
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

    /// Swaps the grid positions of two nodes by reordering them in the backing Vec.
    /// Returns `true` if both ids exist and are distinct; otherwise leaves order unchanged and returns `false`.
    pub fn swap_node_order(&mut self, a: NodeId, b: NodeId) -> bool {
        let pos_a = self.nodes.iter().position(|n| n.id == a);
        let pos_b = self.nodes.iter().position(|n| n.id == b);
        match (pos_a, pos_b) {
            (Some(i), Some(j)) if i != j => {
                self.nodes.swap(i, j);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;
