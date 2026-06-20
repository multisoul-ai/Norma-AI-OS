//! 画布节点数据模型（零 gpui）。

use crate::geometry::CanvasRect;

pub type NodeId = u64;

/// 普通矩形节点（本切片用于验证拖拽/选中）。color 为 0xRRGGBB。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectNode {
    pub color: u32,
}

/// 指向一个 terminal session 的引用（实际 session 存在 runtime 层）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalNodeRef {
    pub session_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasNodeKind {
    Rect(RectNode),
    Terminal(TerminalNodeRef),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasNode {
    pub id: NodeId,
    pub kind: CanvasNodeKind,
    pub bounds: CanvasRect,
    pub z_index: i32,
    pub selected: bool,
}
