use super::load_preview_content;
use crate::app::{AppScreen, MultisoulApp};
use crate::canvas::master_layout::{
    apply_workspace_reflow_for_area, enter_master_layout_for_area, reapply_master_layout_for_area,
    switch_master_layout_for_area,
};
use canvas::{CanvasNodeKind, CanvasRect, NodeId, PreviewType};
use gpui::{AppContext, Context, PathPromptOptions, Window};
use std::path::PathBuf;

impl MultisoulApp {
    pub(crate) fn open_terminal_path_in_preview_card(
        &mut self,
        relative_path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.runtime.workspace_root() else {
            self.workspace_state.error =
                Some("Open a project before previewing terminal file links.".into());
            return;
        };
        let path = root.join(relative_path);
        let area = self.visible_canvas_rect(window);
        let bounds = area.inset(self.layout_gap_canvas());
        let zoom = self.runtime.document.viewport.zoom;
        let node_id = self.runtime.spawn_preview_from_path(path, None, bounds);
        self.motion.spawn_node(node_id, bounds);
        let arranged = if self.master_layout.is_active() {
            switch_master_layout_for_area(
                &mut self.master_layout,
                &mut self.runtime,
                &mut self.motion,
                node_id,
                area,
                zoom,
            )
        } else {
            enter_master_layout_for_area(
                &mut self.master_layout,
                &mut self.runtime,
                &mut self.motion,
                node_id,
                area,
                zoom,
            )
        };
        if arranged {
            self.mark_canvas_dirty();
            self.load_preview_node(node_id, cx);
        } else {
            self.workspace_state.error = Some("Could not arrange preview link card.".into());
        }
    }

    pub(crate) fn prompt_preview_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let area = self.visible_canvas_rect(window);
        let bounds = area.inset(self.layout_gap_canvas());
        let zoom = self.runtime.document.viewport.zoom;
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Preview File".into()),
        });

        cx.spawn(async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.update(&mut *cx, |app, cx| {
                            app.spawn_preview_from_path(path, None, bounds, area, zoom, cx);
                        })?;
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error =
                            Some(format!("Could not open preview picker: {error}"));
                        cx.notify();
                    })?;
                }
                Err(error) => {
                    this.update(&mut *cx, |app, cx| {
                        app.workspace_state.error = Some(format!(
                            "Preview picker was cancelled unexpectedly: {error}"
                        ));
                        cx.notify();
                    })?;
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    pub(crate) fn spawn_preview_from_path(
        &mut self,
        path: PathBuf,
        explicit_type: Option<PreviewType>,
        bounds: CanvasRect,
        area: CanvasRect,
        zoom: f32,
        cx: &mut Context<Self>,
    ) -> NodeId {
        let node_id = self
            .runtime
            .spawn_preview_from_path(path, explicit_type, bounds);
        self.runtime.selection.select_only(node_id);
        self.runtime.focus.exit_to_canvas();
        self.motion.spawn_node(node_id, bounds);
        self.apply_preview_layout_for_area(area, zoom);
        self.mark_canvas_dirty();
        self.load_preview_node(node_id, cx);
        cx.notify();
        node_id
    }

    pub(crate) fn refresh_preview_node(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        self.load_preview_node(node_id, cx);
    }

    pub(crate) fn reload_all_previews(&mut self, cx: &mut Context<Self>) {
        self.previews.clear();
        let node_ids: Vec<NodeId> = self
            .runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Preview(_)))
            .map(|node| node.id)
            .collect();
        for node_id in node_ids {
            self.load_preview_node(node_id, cx);
        }
    }

    fn load_preview_node(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        let Some((path, explicit_type)) = self.preview_spec_for_node(node_id) else {
            self.previews.remove(node_id);
            return;
        };
        let generation = self.previews.begin_loading(node_id);
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { load_preview_content(path, explicit_type) })
                .await;
            this.update(&mut *cx, |app, cx| {
                if app.previews.finish_loading(node_id, generation, result) {
                    cx.notify();
                }
            })?;
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    fn preview_spec_for_node(&self, node_id: NodeId) -> Option<(PathBuf, Option<PreviewType>)> {
        let node = self.runtime.document.node(node_id)?;
        let CanvasNodeKind::Preview(spec) = &node.kind else {
            return None;
        };
        Some((PathBuf::from(spec.path.clone()), spec.explicit_type))
    }

    pub(crate) fn can_create_preview(&self) -> bool {
        matches!(self.screen, AppScreen::Canvas)
    }

    fn apply_preview_layout_for_area(&mut self, area: CanvasRect, zoom: f32) {
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
        apply_workspace_reflow_for_area(&mut self.runtime, &mut self.motion, area, zoom);
    }
}
