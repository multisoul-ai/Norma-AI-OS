//! canvas — 域层（零 gpui）：几何、节点、选择、命中、视口。
pub mod commands;
pub mod document;
pub mod geometry;
pub mod history;
pub mod node;

pub use document::Document;
pub use geometry::{CanvasRect, Transform2D, Vec2, Viewport};
pub use node::{CanvasNode, CanvasNodeKind, NodeId, RectNode, TerminalNodeRef};
