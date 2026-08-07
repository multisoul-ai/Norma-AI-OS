//! canvas — 域层（零 gpui）：几何、节点、选择、命中、视口。
pub mod commands;
pub mod delete;
pub mod document;
pub mod geometry;
pub mod history;
pub mod hit_test;
pub mod layout;
pub mod navigation;
pub mod node;
pub mod selection;

pub use delete::next_neighbor_after_delete;
pub use document::{CanvasNodeSnapshot, Document, DocumentSnapshot};
pub use geometry::{CanvasRect, Transform2D, Vec2, Viewport};
pub use hit_test::hit_test;
pub use layout::{
    compute_configurable_grid_layout, compute_master_detail_layout, compute_responsive_grid_layout,
    compute_tiled_grid_layout, configurable_grid_cell_bounds, ConfigurableGridConfig,
    GridCardSizing, GridFill, LayoutAssignment, LayoutInsets, ResponsiveGridConfig,
    TiledGridConfig, DEFAULT_CARD_ASPECT_RATIO,
};
pub use navigation::{navigate_card, NavigationDirection};
pub use node::{
    AgentProfile, AgentSpec, BrowserSpec, BrowserSurfaceKind, CanvasNode, CanvasNodeKind,
    EgoCardPhase, EgoLiteSpec, NodeId, NodeKind, NodeType, PreviewSpec, PreviewType, RectNode,
    TermSpec,
};
pub use selection::Selection;
