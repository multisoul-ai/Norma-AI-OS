//! 三列平铺布局：按节点顺序 row-major 排列，超过一行后向下延展。

use crate::geometry::{CanvasRect, Vec2};
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

/// 网格中卡片的填充方向。`RowFirst` 固定列数向下增长；`ColumnFirst` 固定行数向右增长。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridFill {
    RowFirst,
    ColumnFirst,
}

/// 网格卡片的尺寸来源。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridCardSizing {
    Fixed,
    FitViewport,
}

/// 用户可配置的无限网格参数。数值均为 canvas 坐标。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfigurableGridConfig {
    pub fill: GridFill,
    pub track_count: usize,
    pub sizing: GridCardSizing,
    pub aspect_ratio: f32,
    pub fixed_card_width: f32,
    pub gap_canvas: f32,
    pub safe_area: LayoutInsets,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutAssignment {
    pub node_id: NodeId,
    pub bounds: CanvasRect,
}

/// Desktop project regions reserve two rows and at least two columns, in catalog order.
/// Fit a centered grid of 16:9 canvases, retaining unoccupied slots for incomplete rows.
/// This presentation-only grid has no node IDs or Agent layout configuration.
pub fn compute_project_region_layout(count: usize, area: CanvasRect, gap: Vec2) -> Vec<CanvasRect> {
    if count == 0
        || !area.size.x.is_finite()
        || !area.size.y.is_finite()
        || area.size.x <= 0.0
        || area.size.y <= 0.0
    {
        return Vec::new();
    }
    let columns = count.div_ceil(2).max(2);
    let gap_x = gap
        .x
        .max(0.0)
        .min(area.size.x / (2.0 * (columns - 1) as f32));
    let gap_y = gap.y.max(0.0).min(area.size.y / 2.0);
    let aspect_ratio = 16.0 / 9.0;
    let width = ((area.size.x - (columns - 1) as f32 * gap_x) / columns as f32)
        .min((area.size.y - gap_y) / 2.0 * aspect_ratio);
    let height = width / aspect_ratio;
    let origin = area.origin
        + Vec2::new(
            (area.size.x - columns as f32 * width - (columns - 1) as f32 * gap_x) / 2.0,
            (area.size.y - 2.0 * height - gap_y) / 2.0,
        );
    (0..count)
        .map(|index| {
            CanvasRect::new(
                origin.x + (index % columns) as f32 * (width + gap_x),
                origin.y + (index / columns) as f32 * (height + gap_y),
                width,
                height,
            )
        })
        .collect()
}

/// 项目总览固定两行网格中的纵向移动目标（row-major，列规则与
/// `compute_project_region_layout` 一致：columns = ceil(count/2)，至少 2 列）。
/// 同列换行；底行缺格时吸附到该行末项；行边界不环绕；单行布局没有纵向目标。
/// 无选中项时与 `runtime::desktop_navigation::adjacent_target` 对齐：向下从首项开始，向上从末项开始。
pub fn overview_row_target(count: usize, current: Option<usize>, up: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let columns = count.div_ceil(2).max(2);
    let bottom_row = count.saturating_sub(columns);
    if bottom_row == 0 {
        // 顶行已容纳全部项目（count ≤ columns）：没有可换的行。
        return None;
    }
    let index = match current {
        Some(index) if index < count => index,
        None => return Some(if up { count - 1 } else { 0 }),
        Some(_) => return None,
    };
    let row = index / columns;
    let column = index % columns;
    match (row, up) {
        (0, false) => Some(columns + column.min(bottom_row - 1)),
        (1, true) => Some(column),
        _ => None,
    }
}

/// 按用户指定方向排列网格，并始终把全部卡片容纳在当前可视区域中。
pub fn compute_configurable_grid_layout(
    ordered_ids: &[NodeId],
    area: CanvasRect,
    config: ConfigurableGridConfig,
) -> Vec<LayoutAssignment> {
    ordered_ids
        .iter()
        .enumerate()
        .map(|(index, node_id)| LayoutAssignment {
            node_id: *node_id,
            bounds: configurable_grid_cell_bounds_for_count(index, ordered_ids.len(), area, config),
        })
        .collect()
}

/// 返回指定索引在可配置网格中的格子 bounds。
///
/// 因调用方未提供总卡片数，本函数以 `index + 1` 作为最小总数；批量重排应使用
/// `compute_configurable_grid_layout`，以便所有格子根据完整节点集合共同缩放。
pub fn configurable_grid_cell_bounds(
    index: usize,
    area: CanvasRect,
    config: ConfigurableGridConfig,
) -> CanvasRect {
    configurable_grid_cell_bounds_for_count(index, index + 1, area, config)
}

fn configurable_grid_cell_bounds_for_count(
    index: usize,
    total_count: usize,
    area: CanvasRect,
    config: ConfigurableGridConfig,
) -> CanvasRect {
    let usable = usable_area(area, config.safe_area);
    let tracks = config.track_count.max(1);
    let gap = config.gap_canvas.max(0.0);
    let aspect_ratio = config.aspect_ratio.max(0.001);
    let total_count = total_count.max(index + 1).max(1);
    let (rows, columns) = match config.fill {
        GridFill::RowFirst => (total_count.div_ceil(tracks), tracks),
        GridFill::ColumnFirst => (tracks, total_count.div_ceil(tracks)),
    };
    let max_width_by_columns =
        ((usable.size.x - gap * (columns as f32 + 1.0)) / columns as f32).max(0.0);
    let max_height_by_rows = ((usable.size.y - gap * (rows as f32 + 1.0)) / rows as f32).max(0.0);
    let (card_width, card_height) = match config.sizing {
        GridCardSizing::Fixed => {
            let viewport_width = max_width_by_columns.min(max_height_by_rows * aspect_ratio);
            let width = config.fixed_card_width.max(0.0).min(viewport_width);
            (width, width / aspect_ratio)
        }
        GridCardSizing::FitViewport => (max_width_by_columns, max_height_by_rows),
    };
    let (row, col) = match config.fill {
        GridFill::RowFirst => (index / tracks, index % tracks),
        GridFill::ColumnFirst => (index % tracks, index / tracks),
    };
    let x = usable.origin.x + gap + col as f32 * (card_width + gap);
    let y = usable.origin.y + gap + row as f32 * (card_height + gap);

    CanvasRect::new(x, y, card_width, card_height)
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

#[cfg(test)]
#[path = "../tests/unit/layout_configurable_grid_tests.rs"]
mod configurable_grid_tests;
