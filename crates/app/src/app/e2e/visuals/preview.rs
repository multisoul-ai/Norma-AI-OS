use crate::app::MultisoulApp;
use canvas::{CanvasNodeKind, CanvasRect, NodeId};
use gpui::Context;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct PreviewImageE2eSnapshot {
    pub index: usize,
    pub image_count: usize,
    pub zoom: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub current_path: Option<String>,
    pub pan_active: bool,
}

impl MultisoulApp {
    pub fn spawn_preview_gallery_for_e2e(
        &mut self,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Option<NodeId> {
        let area = CanvasRect::new(0.0, 0.0, 1200.0, 800.0);
        let bounds = CanvasRect::new(180.0, 120.0, 700.0, 500.0);
        let zoom = self.runtime.document.viewport.zoom;
        self.spawn_preview_from_paths(paths, None, bounds, area, zoom, cx)
    }

    pub fn preview_image_e2e_snapshot(&self, node_id: NodeId) -> Option<PreviewImageE2eSnapshot> {
        let presentation = self.previews.image_viewer_presentation(node_id)?;
        let node = self.runtime.document.node(node_id)?;
        let CanvasNodeKind::Preview(spec) = &node.kind else {
            return None;
        };
        let offset = presentation.scroll_handle.offset();
        let current_path = spec
            .all_paths()
            .nth(presentation.index)
            .map(ToString::to_string);
        Some(PreviewImageE2eSnapshot {
            index: presentation.index,
            image_count: presentation.image_count,
            zoom: presentation.zoom,
            offset_x: f32::from(offset.x),
            offset_y: f32::from(offset.y),
            current_path,
            pan_active: self
                .interaction
                .preview_pan
                .map(|pan| pan.node_id == node_id)
                .unwrap_or(false),
        })
    }
}
