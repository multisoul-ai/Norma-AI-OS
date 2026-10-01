//! 用户可配置的画布网格策略，以及将全部卡片保持在可视区域内的应用层桥接。

use crate::cards::card_for;
use crate::motion::MotionStore;
use canvas::{
    compute_configurable_grid_layout, configurable_grid_cell_bounds, CanvasRect,
    ConfigurableGridConfig, GridCardSizing, GridFill, LayoutAssignment, LayoutInsets, NodeId,
    NodeType, DEFAULT_CARD_ASPECT_RATIO,
};
use runtime::Runtime;

const LAYOUT_GAP_SCREEN_PX: f32 = 8.0;
const FIXED_CARD_WIDTH_SCREEN_PX: f32 = 360.0;
const WORKSPACE_SAFE_LEFT_PX: f32 = 96.0;
const WORKSPACE_SAFE_TOP_PX: f32 = 86.0;
const WORKSPACE_SAFE_RIGHT_PX: f32 = 32.0;
const WORKSPACE_SAFE_BOTTOM_PX: f32 = 84.0;

/// 底部布局菜单中的策略。新增卡片会与现有卡片共同缩放，以保持在当前视区内。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CanvasLayoutStrategy {
    pub(crate) fill: GridFill,
    pub(crate) track_count: usize,
    pub(crate) sizing: GridCardSizing,
    pub(crate) chrome_in_parent: bool,
}

impl Default for CanvasLayoutStrategy {
    fn default() -> Self {
        Self {
            fill: GridFill::RowFirst,
            track_count: 3,
            sizing: GridCardSizing::FitViewport,
            chrome_in_parent: false,
        }
    }
}

impl CanvasLayoutStrategy {
    pub(crate) fn track_label(self) -> &'static str {
        match self.fill {
            GridFill::RowFirst => "每行卡片数",
            GridFill::ColumnFirst => "每列卡片数",
        }
    }

    pub(crate) fn grid_config_for_zoom(self, zoom: f32) -> ConfigurableGridConfig {
        let zoom = zoom.max(0.001);
        ConfigurableGridConfig {
            fill: self.fill,
            track_count: self.track_count.max(1),
            sizing: self.sizing,
            aspect_ratio: DEFAULT_CARD_ASPECT_RATIO,
            fixed_card_width: FIXED_CARD_WIDTH_SCREEN_PX / zoom,
            gap_canvas: LAYOUT_GAP_SCREEN_PX / zoom,
            safe_area: workspace_safe_area_for_zoom(zoom, self.chrome_in_parent),
        }
    }

    pub(crate) fn increase_track_count(&mut self) {
        self.track_count = self.track_count.saturating_add(1).min(12);
    }

    pub(crate) fn decrease_track_count(&mut self) {
        self.track_count = self.track_count.saturating_sub(1).max(1);
    }
}

pub(crate) fn layout_gap_for_zoom(zoom: f32) -> f32 {
    LAYOUT_GAP_SCREEN_PX / zoom.max(0.001)
}

pub(crate) fn workspace_safe_area_for_zoom(zoom: f32, chrome_in_parent: bool) -> LayoutInsets {
    if chrome_in_parent {
        return LayoutInsets::ZERO;
    }
    LayoutInsets::new(
        WORKSPACE_SAFE_LEFT_PX / zoom,
        WORKSPACE_SAFE_TOP_PX / zoom,
        WORKSPACE_SAFE_RIGHT_PX / zoom,
        WORKSPACE_SAFE_BOTTOM_PX / zoom,
    )
}

pub(crate) fn workspace_bounds_for_index(
    index: usize,
    area: CanvasRect,
    zoom: f32,
    strategy: CanvasLayoutStrategy,
) -> CanvasRect {
    configurable_grid_cell_bounds(index, area, strategy.grid_config_for_zoom(zoom))
}

pub(crate) fn apply_workspace_reflow_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    area: CanvasRect,
    zoom: f32,
    strategy: CanvasLayoutStrategy,
) {
    let ordered_ids: Vec<_> = runtime
        .document
        .nodes()
        .iter()
        .map(|node| node.id)
        .collect();
    let assignments =
        compute_configurable_grid_layout(&ordered_ids, area, strategy.grid_config_for_zoom(zoom));
    apply_layout_assignments(runtime, motion, assignments, zoom);
}

/// 新增节点后重新计算全部格子，避免超过固定行/列数后向当前窗口外溢出。
pub(crate) fn apply_new_workspace_node_layout_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    node_id: NodeId,
    area: CanvasRect,
    zoom: f32,
    strategy: CanvasLayoutStrategy,
) -> bool {
    let exists = runtime
        .document
        .nodes()
        .iter()
        .any(|node| node.id == node_id);
    if !exists {
        return false;
    }
    apply_workspace_reflow_for_area(runtime, motion, area, zoom, strategy);
    true
}

fn apply_layout_assignments(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    assignments: Vec<LayoutAssignment>,
    zoom: f32,
) {
    for assignment in assignments {
        let node_type = runtime.document.node(assignment.node_id).map(|node| {
            motion.ensure_node(assignment.node_id, node.bounds);
            node.kind.node_type()
        });
        runtime
            .document
            .set_node_bounds(assignment.node_id, assignment.bounds);
        motion.set_layout_target(assignment.node_id, assignment.bounds);
        resize_node_for_bounds(
            runtime,
            assignment.node_id,
            node_type,
            assignment.bounds,
            zoom,
        );
    }
}

fn resize_node_for_bounds(
    runtime: &mut Runtime,
    node_id: NodeId,
    node_type: Option<NodeType>,
    bounds: CanvasRect,
    zoom: f32,
) {
    if let Some(resize) = node_type.and_then(|node_type| card_for(node_type).resize(bounds, zoom)) {
        let _ = runtime.resize_node(node_id, resize);
    }
}
