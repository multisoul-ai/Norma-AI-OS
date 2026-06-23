//! 三列平铺布局：按节点顺序 row-major 排列，超过一行后向下延展。

use crate::geometry::CanvasRect;
use crate::node::NodeId;

const BASE_CARD_ASPECT_RATIO: f32 = 16.0 / 10.0;
const DEFAULT_CARD_HEIGHT_MULTIPLIER: f32 = 1.4 * 0.9;
const MASTER_DETAIL_MASTER_WIDTH_RATIO: f32 = 2.0 / 3.0;
pub const DEFAULT_CARD_ASPECT_RATIO: f32 = BASE_CARD_ASPECT_RATIO / DEFAULT_CARD_HEIGHT_MULTIPLIER;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TiledGridConfig {
    pub max_columns: usize,
    pub aspect_ratio: f32,
    pub gap_canvas: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl LayoutInsets {
    pub const ZERO: LayoutInsets = LayoutInsets {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        LayoutInsets {
            left,
            top,
            right,
            bottom,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResponsiveGridConfig {
    pub max_columns: usize,
    pub min_column_width: f32,
    pub aspect_ratio: f32,
    pub gap_canvas: f32,
    pub safe_area: LayoutInsets,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutAssignment {
    pub node_id: NodeId,
    pub bounds: CanvasRect,
}

pub fn compute_tiled_grid_layout(
    ordered_ids: &[NodeId],
    area: CanvasRect,
    config: TiledGridConfig,
) -> Vec<LayoutAssignment> {
    if ordered_ids.is_empty() {
        return Vec::new();
    }

    let max_columns = config.max_columns.max(1);
    let columns = ordered_ids.len().min(max_columns);
    let gap = config.gap_canvas.max(0.0);
    let aspect_ratio = config.aspect_ratio.max(0.001);
    let tile_width = ((area.size.x - gap * (columns as f32 + 1.0)) / columns as f32).max(0.0);
    let tile_height = tile_width / aspect_ratio;

    ordered_ids
        .iter()
        .enumerate()
        .map(|(index, node_id)| {
            let row = index / columns;
            let col = index % columns;
            let x = area.origin.x + gap + col as f32 * (tile_width + gap);
            let y = area.origin.y + gap + row as f32 * (tile_height + gap);

            LayoutAssignment {
                node_id: *node_id,
                bounds: CanvasRect::new(x, y, tile_width, tile_height),
            }
        })
        .collect()
}

pub fn compute_responsive_grid_layout(
    ordered_ids: &[NodeId],
    area: CanvasRect,
    config: ResponsiveGridConfig,
) -> Vec<LayoutAssignment> {
    if ordered_ids.is_empty() {
        return Vec::new();
    }

    let usable = usable_area(area, config.safe_area);

    let max_columns = config.max_columns.max(1).min(ordered_ids.len());
    let gap = config.gap_canvas.max(0.0);
    let min_column_width = config.min_column_width.max(1.0);
    let mut columns = 1;
    for candidate in 1..=max_columns {
        let available = usable.size.x - gap * (candidate as f32 + 1.0);
        let candidate_width = available / candidate as f32;
        if candidate_width >= min_column_width {
            columns = candidate;
        }
    }

    let rows = ordered_ids.len().div_ceil(columns);
    let tile_width = ((usable.size.x - gap * (columns as f32 + 1.0)) / columns as f32).max(0.0);
    let tile_height = ((usable.size.y - gap * (rows as f32 + 1.0)) / rows as f32).max(0.0);

    ordered_ids
        .iter()
        .enumerate()
        .map(|(index, node_id)| {
            let row = index / columns;
            let col = index % columns;
            let x = usable.origin.x + gap + col as f32 * (tile_width + gap);
            let y = usable.origin.y + gap + row as f32 * (tile_height + gap);

            LayoutAssignment {
                node_id: *node_id,
                bounds: CanvasRect::new(x, y, tile_width, tile_height),
            }
        })
        .collect()
}

pub fn compute_master_detail_layout(
    ordered_ids: &[NodeId],
    master_id: NodeId,
    area: CanvasRect,
    config: ResponsiveGridConfig,
) -> Vec<LayoutAssignment> {
    if !ordered_ids.contains(&master_id) {
        return Vec::new();
    }

    let usable = usable_area(area, config.safe_area);
    let gap = config.gap_canvas.max(0.0);
    let content_width = (usable.size.x - gap * 3.0).max(0.0);
    let master_width = content_width * MASTER_DETAIL_MASTER_WIDTH_RATIO;
    let panel_height = (usable.size.y - gap * 2.0).max(0.0);
    let master_bounds = CanvasRect::new(
        usable.origin.x + gap,
        usable.origin.y + gap,
        master_width,
        panel_height,
    );

    let detail_ids: Vec<NodeId> = ordered_ids
        .iter()
        .copied()
        .filter(|id| *id != master_id)
        .collect();
    let detail_area = CanvasRect::new(
        master_bounds.max().x,
        usable.origin.y,
        (usable.max().x - master_bounds.max().x).max(0.0),
        usable.size.y,
    );
    let detail_config = ResponsiveGridConfig {
        safe_area: LayoutInsets::ZERO,
        ..config
    };

    let mut assignments = vec![LayoutAssignment {
        node_id: master_id,
        bounds: master_bounds,
    }];
    assignments.extend(compute_responsive_grid_layout(
        &detail_ids,
        detail_area,
        detail_config,
    ));
    assignments
}

fn usable_area(area: CanvasRect, safe_area: LayoutInsets) -> CanvasRect {
    let safe_left = safe_area.left.max(0.0);
    let safe_top = safe_area.top.max(0.0);
    let safe_right = safe_area.right.max(0.0);
    let safe_bottom = safe_area.bottom.max(0.0);
    CanvasRect::new(
        area.origin.x + safe_left,
        area.origin.y + safe_top,
        (area.size.x - safe_left - safe_right).max(0.0),
        (area.size.y - safe_top - safe_bottom).max(0.0),
    )
}

#[cfg(test)]
#[path = "../tests/unit/layout_tiled_tests.rs"]
mod tiled_tests;

#[cfg(test)]
#[path = "../tests/unit/layout_responsive_tests.rs"]
mod responsive_tests;

#[cfg(test)]
#[path = "../tests/unit/layout_master_detail_tests.rs"]
mod master_detail_tests;
