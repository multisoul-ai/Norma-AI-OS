//! 画布布局策略的应用入口：显式重排全部卡片，或只为刚创建的卡片写入目标格。

use super::MultisoulApp;
use crate::canvas::layout_strategy::{
    apply_new_workspace_node_layout_for_area, apply_workspace_reflow_for_area, layout_gap_for_zoom,
    workspace_bounds_for_index,
};
use crate::canvas::master_layout::reapply_master_layout_for_area;
use canvas::{CanvasRect, NodeId};
use gpui::Window;

impl MultisoulApp {
    pub(crate) fn layout_gap_canvas(&self) -> f32 {
        layout_gap_for_zoom(self.runtime.document.viewport.zoom)
    }

    fn apply_tiled_nodes(&mut self, window: &mut Window) {
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        apply_workspace_reflow_for_area(
            &mut self.runtime,
            &mut self.motion,
            area,
            zoom,
            self.layout_strategy,
        );
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
        workspace_bounds_for_index(
            self.runtime.document.nodes().len(),
            area,
            zoom,
            self.layout_strategy,
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
            self.layout_strategy,
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
