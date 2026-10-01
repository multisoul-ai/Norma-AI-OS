use crate::canvas::master_layout::workspace_reflow_config_for_zoom;
use canvas::{compute_responsive_grid_layout, CanvasRect, LayoutInsets, NodeId};

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
