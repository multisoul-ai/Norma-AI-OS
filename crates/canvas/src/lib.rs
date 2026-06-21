//! canvas — 域层（零 gpui）：几何、节点、选择、命中、视口。
pub mod commands;
pub mod document;
pub mod geometry;
pub mod history;
pub mod hit_test;
pub mod layout;
pub mod node;
pub mod selection;

pub use document::Document;
pub use geometry::{CanvasRect, Transform2D, Vec2, Viewport};
pub use hit_test::hit_test;
pub use layout::{
    compute_responsive_grid_layout, compute_tiled_grid_layout, LayoutAssignment, LayoutInsets,
    ResponsiveGridConfig, TiledGridConfig, DEFAULT_CARD_ASPECT_RATIO,
};
pub use node::{
    AgentProfile, AgentSpec, CanvasNode, CanvasNodeKind, NodeId, NodeKind, NodeType, RectNode,
    TermSpec,
};
pub use selection::Selection;
