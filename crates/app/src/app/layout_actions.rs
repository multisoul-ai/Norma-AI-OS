//! 画布布局策略的应用入口：显式重排全部卡片，或只为刚创建的卡片写入目标格。

use super::MultisoulApp;
use crate::canvas::layout_strategy::{
    apply_embedded_project_reflow_for_area, apply_new_workspace_node_layout_for_area,
    apply_workspace_reflow_for_area, layout_gap_for_zoom, workspace_bounds_for_index,
};
use crate::canvas::master_layout::reapply_master_layout_for_area;
use canvas::{CanvasRect, NodeId, Vec2};
use gpui::Window;

impl MultisoulApp {
    fn stable_embedded_layout_area(&self, current: CanvasRect) -> CanvasRect {
        let Some(size) = self.desktop_embedded_reflow_size else {
            return current;
        };
        let viewport = self.runtime.document.viewport;
        let top_left = self
            .desktop_embedded_layout_origin
            .unwrap_or_else(|| viewport.screen_to_canvas(Vec2::ZERO));
        // A small App window must not persist its temporary single-column presentation.
        // Keep enough project space for the default three-column layout; drawers and window
        // width still determine the visible presentation in embedded_canvas.rs.
        let stable_size = Vec2::new(size.x.max(945.0), size.y.max(645.0));
        CanvasRect::new(
            top_left.x,
            top_left.y,
            stable_size.x / viewport.zoom.max(0.001),
            stable_size.y / viewport.zoom.max(0.001),
        )
    }

    fn effective_layout_strategy(
        &self,
        area: CanvasRect,
        zoom: f32,
    ) -> crate::canvas::layout_strategy::CanvasLayoutStrategy {
        if self.desktop_host.is_some() {
            crate::canvas::layout_strategy::embedded_project_layout_strategy(
                self.layout_strategy,
                area.size.x * zoom,
            )
        } else {
            self.layout_strategy
        }
    }

    pub(crate) fn layout_gap_canvas(&self) -> f32 {
        layout_gap_for_zoom(self.runtime.document.viewport.zoom)
    }

    fn apply_tiled_nodes(&mut self, window: &mut Window) {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let layout_area = if self.desktop_host.is_some() {
            self.stable_embedded_layout_area(area)
        } else {
            area
        };
        let strategy = self.effective_layout_strategy(layout_area, zoom);
        if self.desktop_host.is_some() {
            apply_embedded_project_reflow_for_area(
                &mut self.runtime,
                &mut self.motion,
                layout_area,
                zoom,
                strategy,
            );
            self.refresh_desktop_presentation_for_area(area, zoom);
        } else {
            apply_workspace_reflow_for_area(
                &mut self.runtime,
                &mut self.motion,
                area,
                zoom,
                strategy,
            );
        }
    }

    pub(crate) fn next_tiled_node_bounds(&self, window: &mut Window) -> CanvasRect {
        self.next_tiled_node_bounds_for_area(
            self.visible_canvas_rect(window),
            self.runtime.document.viewport.zoom,
        )
    }

    pub(crate) fn next_tiled_node_bounds_for_area(
        &self,
        area: CanvasRect,
        zoom: f32,
    ) -> CanvasRect {
        let layout_area = if self.desktop_host.is_some() {
            self.stable_embedded_layout_area(area)
        } else {
            area
        };
        let node_count = self
            .runtime
            .document
            .nodes()
            .iter()
            .filter(|node| {
                self.desktop_host.is_none()
                    || crate::canvas::view::embedded_node_visible(node.kind.node_type(), true)
            })
            .count();
        workspace_bounds_for_index(
            node_count,
            layout_area,
            zoom,
            self.effective_layout_strategy(layout_area, zoom),
        )
    }

    pub(crate) fn reflow_tiled_nodes(&mut self, window: &mut Window) {
        self.master_layout.clear();
        self.apply_tiled_nodes(window);
    }

    pub(crate) fn apply_layout_for_new_node(&mut self, node_id: NodeId, window: &mut Window) {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        self.apply_layout_for_new_node_for_area(node_id, area, zoom);
    }

    pub(crate) fn apply_layout_for_new_node_for_area(
        &mut self,
        node_id: NodeId,
        area: CanvasRect,
        zoom: f32,
    ) {
        if self.desktop_host.is_some() {
            self.master_layout.clear();
            let layout_area = self.stable_embedded_layout_area(area);
            let strategy = self.effective_layout_strategy(layout_area, zoom);
            apply_embedded_project_reflow_for_area(
                &mut self.runtime,
                &mut self.motion,
                layout_area,
                zoom,
                strategy,
            );
            self.refresh_desktop_presentation_for_area(area, zoom);
            return;
        }
        let strategy = self.effective_layout_strategy(area, zoom);
        if self.master_layout.is_active()
            && reapply_master_layout_for_area(
                &mut self.master_layout,
                &mut self.runtime,
                &mut self.motion,
                area,
                zoom,
            )
        {
            return;
        }
        apply_new_workspace_node_layout_for_area(
            &mut self.runtime,
            &mut self.motion,
            node_id,
            area,
            zoom,
            strategy,
        );
    }

    pub(crate) fn apply_layout_for_current_mode(&mut self, window: &mut Window) {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        if self.master_layout.is_active() {
            let _ = reapply_master_layout_for_area(
                &mut self.master_layout,
                &mut self.runtime,
                &mut self.motion,
                area,
                zoom,
            );
            return;
        }
        self.apply_tiled_nodes(window);
    }
}
