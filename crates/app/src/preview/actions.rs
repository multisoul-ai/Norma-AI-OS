use super::terminal_path::resolve_terminal_preview_path;
use super::{load_preview_contents, PreviewImageCommand, PreviewImageCommandKind};
use crate::app::{AppScreen, MultisoulApp};
use crate::canvas::layout_strategy::apply_new_workspace_node_layout_for_area;
use crate::canvas::master_layout::{
    enter_master_layout_for_area, reapply_master_layout_for_area, switch_master_layout_for_area,
};
use canvas::{CanvasNodeKind, CanvasRect, NodeId, PreviewType};
use futures::channel::oneshot;
use gpui::{AppContext, Context, PathPromptOptions, Window};
use std::path::PathBuf;

impl MultisoulApp {
    pub(crate) fn on_preview_image_command(
        &mut self,
        action: &PreviewImageCommand,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action.command {
            PreviewImageCommandKind::BeginPan { screen_x, screen_y } => {
                if self.previews.image_can_pan(action.node_id) {
                    self.runtime.selection.select_only(action.node_id);
                    self.runtime.focus.exit_to_canvas();
                    self.mark_node_viewed_if_needed(action.node_id);
                    self.interaction
                        .begin_preview_pan(action.node_id, canvas::Vec2::new(screen_x, screen_y));
                    cx.notify();
                }
            }
            command => {
                if self.previews.apply_image_command(action.node_id, command) {
                    cx.notify();
                }
            }
        }
    }

    pub(crate) fn open_terminal_path_in_preview_card(
        &mut self,
        targets: Vec<String>,
        source_node: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.runtime.workspace_root() else {
            self.workspace_state.error = Some(ui::i18n::LocalizedText::new("preview.open_project"));
            return;
        };
        let root = root.to_path_buf();
        let lookup_root = root.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(
                    async move { resolve_terminal_preview_path(&lookup_root, &targets) },
                )
                .await;
            this.update_in(cx, |app, window, cx| {
                if app.runtime.workspace_root() != Some(root.as_path())
                    || app.runtime.document.node(source_node).is_none()
                    || !app.can_create_preview()
                {
                    return;
                }
                match result {
                    Ok(path) => {
                        app.workspace_state.error = None;
                        app.open_resolved_terminal_preview(path, window, cx);
                    }
                    Err(error) => {
                        app.workspace_state.error = Some(
                            ui::i18n::LocalizedText::new("common.operation_failed")
                                .with_arg("reason", error),
                        )
                    }
                }
                cx.notify();
            })?;
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }

    fn open_resolved_terminal_preview(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("文件")
            .to_string();
        if self.route_project_content_to_desktop(
            storage::DesktopContentKind::File,
            path.to_string_lossy().into_owned(),
            title,
            cx,
        ) {
            return;
        }
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
            self.workspace_state.error =
                Some(ui::i18n::LocalizedText::new("preview.arrange_failed"));
        }
    }

    pub(crate) fn prompt_preview_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prompt_preview_file_with_source(false, window, cx);
    }

    pub(crate) fn prompt_preview_file_from_command(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt_preview_file_with_source(true, window, cx);
    }

    fn prompt_preview_file_with_source(
        &mut self,
        from_command: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(ui::i18n::text("preview.picker_title").into()),
        });
        self.handle_preview_picker_receiver(receiver, from_command, window, cx);
    }

    pub(crate) fn handle_preview_picker_receiver(
        &mut self,
        receiver: oneshot::Receiver<anyhow::Result<Option<Vec<PathBuf>>>>,
        from_command: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let area = self.visible_canvas_rect(window);
        let bounds = self.next_tiled_node_bounds(window);
        let zoom = self.runtime.document.viewport.zoom;
        let workspace = self.runtime.workspace_root().map(PathBuf::from);
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await;
            this.update_in(cx, |app, window, cx| {
                if app.runtime.workspace_root() != workspace.as_deref() || !app.can_create_preview()
                {
                    if from_command {
                        if let Some(state) = &mut app.command_dock {
                            state.pending_preview = None;
                        }
                    }
                    cx.notify();
                    return;
                }
                let mut success = false;
                let mut picker_error = None;
                match result {
                    Ok(Ok(Some(paths))) if !paths.is_empty() => {
                        if app.desktop_host.is_some() {
                            success = paths.into_iter().all(|path| {
                                let title = path
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("文件")
                                    .to_string();
                                app.route_project_content_to_desktop(
                                    storage::DesktopContentKind::File,
                                    path.to_string_lossy().into_owned(),
                                    title,
                                    cx,
                                )
                            });
                        } else {
                            success = app
                                .spawn_preview_from_paths(paths, None, bounds, area, zoom, cx)
                                .is_some();
                        }
                        if !success {
                            picker_error = app.workspace_state.error.clone();
                        }
                    }
                    Ok(Err(error)) => {
                        picker_error = Some(
                            ui::i18n::LocalizedText::new("cards.preview_picker_failed")
                                .with_arg("error", error),
                        )
                    }
                    Err(error) => {
                        picker_error = Some(
                            ui::i18n::LocalizedText::new("cards.preview_picker_cancelled")
                                .with_arg("error", error),
                        )
                    }
                    _ => {}
                }
                if let Some(error) = &picker_error {
                    app.workspace_state.error = Some(error.clone());
                }
                if from_command {
                    app.finish_command_preview(success, picker_error, window, cx);
                }
                cx.notify();
            })?;
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
        self.finish_preview_spawn(node_id, bounds, area, zoom, cx)
    }

    pub(crate) fn spawn_preview_from_paths(
        &mut self,
        paths: Vec<PathBuf>,
        explicit_type: Option<PreviewType>,
        bounds: CanvasRect,
        area: CanvasRect,
        zoom: f32,
        cx: &mut Context<Self>,
    ) -> Option<NodeId> {
        let node_id = match self
            .runtime
            .spawn_preview_from_paths(paths, explicit_type, bounds)
        {
            Ok(node_id) => node_id,
            Err(error) => {
                self.workspace_state.error = Some(
                    ui::i18n::LocalizedText::new("common.operation_failed")
                        .with_arg("reason", error),
                );
                cx.notify();
                return None;
            }
        };
        Some(self.finish_preview_spawn(node_id, bounds, area, zoom, cx))
    }

    fn finish_preview_spawn(
        &mut self,
        node_id: NodeId,
        bounds: CanvasRect,
        area: CanvasRect,
        zoom: f32,
        cx: &mut Context<Self>,
    ) -> NodeId {
        self.runtime.selection.select_only(node_id);
        self.runtime.focus.exit_to_canvas();
        self.motion.spawn_node(node_id, bounds);
        self.apply_preview_layout_for_new_node(node_id, area, zoom);
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
        if self.previews.has_pending_edits(node_id) {
            if let Some(editor) = self.previews.editors.get_mut(&node_id) {
                editor.error = Some(ui::i18n::LocalizedText::new("preview.refresh_unsaved"));
            }
            cx.notify();
            return;
        }
        self.previews.editors.remove(&node_id);
        let Some((paths, explicit_type)) = self.preview_spec_for_node(node_id) else {
            self.previews.remove(node_id);
            return;
        };
        let generation = self.previews.begin_loading(node_id);
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { load_preview_contents(paths, explicit_type) })
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

    pub(super) fn preview_spec_for_node(
        &self,
        node_id: NodeId,
    ) -> Option<(Vec<PathBuf>, Option<PreviewType>)> {
        let node = self.runtime.document.node(node_id)?;
        let CanvasNodeKind::Preview(spec) = &node.kind else {
            return None;
        };
        Some((
            spec.all_paths().map(PathBuf::from).collect(),
            spec.explicit_type,
        ))
    }

    pub(crate) fn can_create_preview(&self) -> bool {
        matches!(self.screen, AppScreen::Canvas)
    }

    fn apply_preview_layout_for_new_node(&mut self, node_id: NodeId, area: CanvasRect, zoom: f32) {
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
}
