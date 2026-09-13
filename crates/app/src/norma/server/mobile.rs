use super::{apply_layout_for_new_node_or_master, resolve_bounds, MultisoulApp, NodeId, Placement};
use gpui::{Context, Window};

impl MultisoulApp {
    pub(crate) fn delete_mobile_card_in_canvas(
        &mut self,
        expected: &norma::mobile::CardSnapshot,
        confirm_busy: bool,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> Result<NodeId, &'static str> {
        let node = expected.node_id;
        // Queued dispatch has its own lifecycle; do not drop tasks when closing a card.
        if self.mobile_node_has_pending_work(node) {
            return Err("state_not_ready");
        }
        self.runtime
            .validate_mobile_card_deletion(expected, confirm_busy)?;
        let area = self.mobile_canvas_area(window);
        let zoom = self.runtime.document.viewport.zoom;
        super::delete_card(
            &mut self.runtime,
            &mut self.motion,
            &mut self.master_layout,
            self.layout_strategy,
            node,
            area,
            zoom,
        )
        .map_err(|_| "not_found")?;
        self.terminal_render_cache.borrow_mut().remove_node(node);
        self.previews.remove(node);
        // Keep Agent definitions and Provider transcripts; this removes only the live card.
        self.mark_canvas_dirty();
        self.mark_session_dirty();
        cx.notify();
        Ok(node)
    }

    pub(crate) fn create_mobile_card_in_canvas(
        &mut self,
        profile: canvas::AgentProfile,
        title: Option<String>,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> Result<NodeId, terminal::PtyError> {
        let area = self.mobile_canvas_area(window);
        let zoom = self.runtime.document.viewport.zoom;
        let bounds = resolve_bounds(
            Placement::VisibleArea,
            area,
            zoom,
            self.layout_strategy,
            self.runtime.document.nodes().len(),
        );
        let node = self
            .runtime
            .spawn_mobile_agent_card(profile, title, bounds, 80, 24)?;
        apply_layout_for_new_node_or_master(
            &mut self.master_layout,
            &mut self.runtime,
            &mut self.motion,
            node,
            area,
            zoom,
            self.layout_strategy,
        );
        self.mark_canvas_dirty();
        self.mark_session_dirty();
        cx.notify();
        Ok(node)
    }

    pub(crate) fn mobile_canvas_area(&self, window: Option<&mut Window>) -> canvas::CanvasRect {
        window
            .map(|window| self.visible_canvas_rect(window))
            .unwrap_or_else(|| {
                // No phone dimensions enter the desktop layout. Reuse the normal initial canvas area.
                let (width, height) = crate::workspace::windowing::initial_window_dimensions(None);
                let viewport = &self.runtime.document.viewport;
                let origin = viewport.screen_to_canvas(canvas::Vec2::new(0.0, 0.0));
                canvas::CanvasRect::new(
                    origin.x,
                    origin.y,
                    width / viewport.zoom,
                    height / viewport.zoom,
                )
            })
    }
}
