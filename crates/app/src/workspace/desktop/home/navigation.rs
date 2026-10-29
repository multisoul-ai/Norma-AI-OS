use super::*;

impl MultisoulApp {
    pub(in crate::workspace::desktop) fn desktop_workspace_directory_rect(
        &mut self,
        workspace_id: &str,
        window: &Window,
    ) -> Option<CanvasRect> {
        let viewport = window.viewport_size();
        let full = CanvasRect::new(0.0, 0.0, viewport.width.into(), viewport.height.into());
        if !self
            .desktop_workspace
            .filtered()
            .iter()
            .any(|item| item.id == workspace_id)
        {
            // A keyboard target can be outside the search results. Establish its real
            // directory slot before zooming; a fabricated center rectangle has no source.
            self.desktop_workspace.search_query.clear();
            self.desktop_workspace.switcher_query.clear();
        }
        let mut workspaces = if self.desktop_workspace.home_directory {
            self.desktop_workspace.workspaces.iter().collect::<Vec<_>>()
        } else {
            self.desktop_workspace.filtered()
        };
        if self.desktop_workspace.home_directory {
            workspaces.sort_by(|a, b| {
                b.last_opened_at_unix_ms
                    .cmp(&a.last_opened_at_unix_ms)
                    .then_with(|| a.id.cmp(&b.id))
            });
        }
        let layout = if self.desktop_workspace.home_directory {
            super::super::layout::home_layout(full.size.x, full.size.y, workspaces.len())
                .into_iter()
                .enumerate()
                .map(|(index, card)| {
                    let mut panel = super::super::layout::home_preview_panel_rect(card, index == 0);
                    // The home card's 1px border offsets its positioned preview child.
                    panel.origin.x += 1.0;
                    panel.origin.y += 1.0;
                    panel
                })
                .collect()
        } else {
            super::super::layout::workspace_layout(full.size.x, full.size.y, workspaces.len())
        };
        let scroll = self.desktop_directory_scroll.offset();
        self.desktop_workspace.workspace_motion_rects = workspaces
            .into_iter()
            .zip(layout)
            .map(|(workspace, mut rect)| {
                rect.origin.x += f32::from(scroll.x);
                rect.origin.y += f32::from(scroll.y);
                (workspace.id.clone(), rect)
            })
            .collect();
        self.desktop_workspace
            .workspace_motion_rects
            .get(workspace_id)
            .copied()
    }
}
