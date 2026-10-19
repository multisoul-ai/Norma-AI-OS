//! Keeps an embedded project's drawing and layout bounds aligned with its workspace slot.

use super::MultisoulApp;
use crate::cards::card_for;
use canvas::{compute_configurable_grid_layout, CanvasNode, CanvasRect, NodeId, Vec2};
use gpui::Window;
use std::collections::HashMap;

/// Draw a responsive view of saved nodes without changing their canvas coordinates.
/// Hidden legacy Browser, Preview and Ego nodes never reserve a grid slot.
pub(crate) fn embedded_presentation_layout(
    nodes: &[CanvasNode],
    area: CanvasRect,
    zoom: f32,
    strategy: crate::canvas::layout_strategy::CanvasLayoutStrategy,
) -> HashMap<NodeId, CanvasRect> {
    let visible: Vec<_> = nodes
        .iter()
        .filter(|node| crate::canvas::view::embedded_node_visible(node.kind.node_type(), true))
        .collect();
    let narrow = area.size.x * zoom <= 650.0;
    let overflow = visible.iter().any(|node| {
        node.bounds.origin.x < area.origin.x
            || node.bounds.origin.x + node.bounds.size.x > area.origin.x + area.size.x
    });
    if !narrow && !overflow {
        return HashMap::new();
    }
    if narrow {
        let gap = 12.0 / zoom.max(0.001);
        // The desktop project scene already supplies the prototype's 14px side padding.
        let width = area.size.x.max(1.0);
        let mut top = area.origin.y;
        return visible
            .into_iter()
            .map(|node| {
                // A narrow canvas scrolls vertically. Do not shrink every live card to fit
                // the viewport height; the prototype keeps its first card nearly full width.
                let height = node
                    .bounds
                    .size
                    .y
                    .min(width * 0.9)
                    .max(180.0 / zoom.max(0.001));
                let bounds = CanvasRect::new(area.origin.x, top, width, height);
                top += height + gap;
                (node.id, bounds)
            })
            .collect();
    }
    let strategy = crate::canvas::layout_strategy::embedded_project_layout_strategy(
        strategy,
        area.size.x * zoom,
    );
    let ids: Vec<_> = visible.iter().map(|node| node.id).collect();
    compute_configurable_grid_layout(&ids, area, strategy.grid_config_for_zoom(zoom))
        .into_iter()
        .map(|assignment| (assignment.node_id, assignment.bounds))
        .collect()
}

/// A stacked presentation needs wheel panning when its full height exceeds the canvas.
pub(crate) fn embedded_presentation_overflows_vertically(
    bounds: &HashMap<NodeId, CanvasRect>,
    viewport_height: f32,
) -> bool {
    let top = bounds
        .values()
        .map(|rect| rect.origin.y)
        .fold(f32::INFINITY, f32::min);
    let bottom = bounds
        .values()
        .map(|rect| rect.origin.y + rect.size.y)
        .fold(f32::NEG_INFINITY, f32::max);
    bottom - top > viewport_height
}

impl MultisoulApp {
    pub(crate) fn visible_canvas_rect(&self, window: &mut Window) -> CanvasRect {
        let viewport = self.runtime.document.viewport;
        let window_size = window.viewport_size();
        let top_left = viewport.screen_to_canvas(Vec2::ZERO);
        let size = if self.desktop_host.is_some() {
            self.desktop_embedded_canvas_size.unwrap_or_else(|| {
                crate::workspace::desktop::desktop_scene_rect(
                    f32::from(window_size.width),
                    f32::from(window_size.height),
                )
                .size
            })
        } else {
            Vec2::new(f32::from(window_size.width), f32::from(window_size.height))
        };
        let bottom_right = viewport.screen_to_canvas(size);
        CanvasRect::new(
            top_left.x,
            top_left.y,
            bottom_right.x - top_left.x,
            bottom_right.y - top_left.y,
        )
    }

    pub(super) fn reflow_if_window_size_changed(&mut self, window: &mut Window) {
        if self.desktop_host.is_some() {
            self.refresh_desktop_presentation(window);
            return;
        }
        let window_size = window.viewport_size();
        let current = self.desktop_embedded_canvas_size.map_or(
            (f32::from(window_size.width), f32::from(window_size.height)),
            |size| (size.x, size.y),
        );
        if self.last_reflow_window_size != Some(current) {
            self.last_reflow_window_size = Some(current);
            self.apply_layout_for_current_mode(window);
        }
    }

    pub(crate) fn sync_desktop_embedded_canvas_size(
        &mut self,
        size: Vec2,
        window: &mut Window,
    ) -> bool {
        self.desktop_embedded_layout_origin
            .get_or_insert_with(|| self.runtime.document.viewport.screen_to_canvas(Vec2::ZERO));
        let viewport = window.viewport_size();
        self.desktop_embedded_reflow_size = Some(
            crate::workspace::desktop::desktop_scene_rect(
                f32::from(viewport.width),
                f32::from(viewport.height),
            )
            .size,
        );
        if self.desktop_embedded_canvas_size == Some(size) {
            return false;
        }
        self.desktop_embedded_canvas_size = Some(size);
        self.last_reflow_window_size = Some((size.x, size.y));
        self.refresh_desktop_presentation(window);
        true
    }

    pub(crate) fn presented_node_bounds(&self, node_id: NodeId) -> Option<CanvasRect> {
        self.desktop_presentation_bounds
            .get(&node_id)
            .copied()
            .or_else(|| self.runtime.document.node(node_id).map(|node| node.bounds))
    }

    fn resize_presented_node_if_live(&mut self, node_id: NodeId, bounds: CanvasRect, zoom: f32) {
        if !self.runtime.node_caps(node_id).term_resize {
            return;
        }
        let Some(node_type) = self
            .runtime
            .document
            .node(node_id)
            .map(|node| node.kind.node_type())
        else {
            return;
        };
        if let Some(resize) = card_for(node_type).resize(bounds, zoom) {
            let _ = self.runtime.resize_node(node_id, resize);
        }
    }

    pub(crate) fn resize_restored_presentation_node(&mut self, node_id: NodeId) {
        if let Some(&bounds) = self.desktop_presentation_bounds.get(&node_id) {
            self.resize_presented_node_if_live(
                node_id,
                bounds,
                self.runtime.document.viewport.zoom,
            );
        }
    }

    pub(crate) fn refresh_desktop_presentation(&mut self, window: &mut Window) {
        if self.desktop_host.is_none() {
            return;
        }
        let area = self.visible_canvas_rect(window);
        self.refresh_desktop_presentation_for_area(area, self.runtime.document.viewport.zoom);
    }

    pub(crate) fn refresh_desktop_presentation_for_area(&mut self, area: CanvasRect, zoom: f32) {
        if self.interaction.drag.is_some() {
            return;
        }
        let anchored_area = CanvasRect {
            origin: self.desktop_embedded_layout_origin.unwrap_or(area.origin),
            size: area.size,
        };
        let next = embedded_presentation_layout(
            self.runtime.document.nodes(),
            anchored_area,
            zoom,
            self.layout_strategy,
        );
        // A layout that fits the slot must start at the slot's top-left. A pan left over
        // from a narrower (scrolling) presentation would otherwise clip the first row.
        if !next.is_empty() && !embedded_presentation_overflows_vertically(&next, area.size.y) {
            if let Some(origin) = self.desktop_embedded_layout_origin {
                self.runtime.document.viewport.pan = Vec2::new(-origin.x * zoom, -origin.y * zoom);
            }
        }
        for (&id, &rect) in &next {
            let presentation_changed = self.desktop_presentation_bounds.get(&id) != Some(&rect);
            // A saved-layout reflow can retarget motion even when this node's compact
            // presentation cell did not change. Always settle its drawn target.
            if presentation_changed {
                self.resize_presented_node_if_live(id, rect, zoom);
            }
            self.motion.settle_presentation_rect(id, rect);
        }
        for id in self
            .desktop_presentation_bounds
            .keys()
            .copied()
            .collect::<Vec<_>>()
        {
            if !next.contains_key(&id) {
                if let Some(bounds) = self.runtime.document.node(id).map(|node| node.bounds) {
                    self.resize_presented_node_if_live(id, bounds, zoom);
                    self.motion.settle_presentation_rect(id, bounds);
                }
            }
        }
        self.desktop_presentation_bounds = next;
    }
}
