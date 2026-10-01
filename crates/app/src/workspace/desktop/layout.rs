use crate::canvas::master_layout::workspace_reflow_config_for_zoom;
use canvas::{compute_responsive_grid_layout, CanvasRect, LayoutInsets, NodeId};

/// Directory cards and camera anchors share this layout even before the first paint.
pub(super) fn workspace_layout(width: f32, height: f32, count: usize) -> Vec<CanvasRect> {
    let available = if width >= 940.0 {
        3
    } else if width >= 610.0 {
        2
    } else {
        1
    };
    let columns = available.min(count.max(1));
    let card_width =
        ((width - 112.0 - 16.0 * (available - 1) as f32) / available as f32).clamp(280.0, 460.0);
    let card_height = height * card_width / width.max(1.0);
    let grid_width = card_width * columns as f32 + 16.0 * (columns - 1) as f32;
    (0..count)
        .map(|index| {
            CanvasRect::new(
                (width - grid_width) * 0.5 + (index % columns) as f32 * (card_width + 16.0),
                118.0 + (index / columns) as f32 * (card_height + 16.0),
                card_width,
                card_height,
            )
        })
        .collect()
}

// Use the settled window size for both the overview and its thumbnail; camera motion only scales it.
pub(super) fn project_area_layout(width: f32, height: f32, count: usize) -> Vec<CanvasRect> {
    let ids: Vec<_> = (0..count as NodeId).collect();
    let mut config = workspace_reflow_config_for_zoom(1.0);
    config.safe_area = LayoutInsets::ZERO;
    compute_responsive_grid_layout(&ids, super::desktop_scene_rect(width, height), config)
        .into_iter()
        .map(|assignment| assignment.bounds)
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_layout_tests.rs"]
mod tests;
