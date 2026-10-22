use super::*;

#[cfg(feature = "e2e-test-support")]
impl MultisoulApp {
    pub fn desktop_wallpaper_render_key_for_e2e(&self) -> String {
        self.wallpaper_settings.render_id_key()
    }

    pub fn desktop_set_catalog_path_for_e2e(&mut self, path: PathBuf) {
        self.desktop_workspace.store = DesktopWorkspaceStore::new(path);
    }

    pub fn desktop_area_node_count_for_e2e(&self, cx: &gpui::App) -> Option<usize> {
        let workspace_id = self.desktop_workspace.active_id.as_ref()?;
        let area_id = self.desktop_workspace.expanded_area_id.as_ref()?;
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.clone(), area_id.clone()))?;
        Some(session.read(cx).runtime.document.nodes().len())
    }

    pub fn desktop_loaded_area_count_for_e2e(&self) -> usize {
        let Some(workspace_id) = self.desktop_workspace.active_id.as_ref() else {
            return 0;
        };
        self.desktop_workspace
            .sessions
            .keys()
            .filter(|(id, _)| id == workspace_id)
            .count()
    }

    pub fn desktop_area_live_count_for_e2e(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &gpui::App,
    ) -> Option<usize> {
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))?;
        Some(session.read(cx).runtime.live_len())
    }

    pub fn desktop_area_running_count_for_e2e(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &gpui::App,
    ) -> Option<usize> {
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))?;
        Some(session.read(cx).runtime.running_terminal_count())
    }

    pub fn desktop_stop_area_runtime_for_e2e(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))
        {
            session.update(cx, |session, _| session.runtime.kill_all());
        }
    }

    pub fn desktop_set_expanded_pty_factory_for_e2e(
        &mut self,
        factory: runtime::PtyFactory,
        cx: &mut gpui::Context<Self>,
    ) {
        if let (Some(workspace_id), Some(area_id)) = (
            self.desktop_workspace.active_id.as_ref(),
            self.desktop_workspace.expanded_area_id.as_ref(),
        ) {
            if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.clone(), area_id.clone()))
            {
                session.update(cx, |session, _| session.runtime.set_pty_factory(factory));
            }
        }
    }

    pub fn desktop_set_area_pty_factory_for_e2e(
        &mut self,
        area_id: &str,
        factory: runtime::PtyFactory,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.as_ref() else {
            return;
        };
        if let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.clone(), area_id.to_owned()))
        {
            session.update(cx, |session, _| session.runtime.set_pty_factory(factory));
        }
    }

    pub fn desktop_expanded_agent_ids_for_e2e(&self, cx: &gpui::App) -> Vec<u64> {
        let (Some(workspace_id), Some(area_id)) = (
            self.desktop_workspace.active_id.as_ref(),
            self.desktop_workspace.expanded_area_id.as_ref(),
        ) else {
            return Vec::new();
        };
        let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.clone(), area_id.clone()))
        else {
            return Vec::new();
        };
        session
            .read(cx)
            .runtime
            .document
            .nodes()
            .iter()
            .filter_map(|node| {
                matches!(node.kind, canvas::CanvasNodeKind::Agent(_)).then_some(node.id)
            })
            .collect()
    }

    pub fn desktop_agent_grid_text_for_e2e(
        &self,
        workspace_id: &str,
        area_id: &str,
        node_id: u64,
        cx: &gpui::App,
    ) -> Option<String> {
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))?;
        let runtime::NodeView::Term { snapshot, .. } =
            session.read(cx).runtime.node_view(node_id)?;
        Some(
            snapshot
                .rows
                .iter()
                .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    pub fn desktop_drain_area_outputs_for_e2e(&mut self, cx: &mut gpui::Context<Self>) {
        for session in self.desktop_workspace.sessions.values() {
            session.update(cx, |session, cx| {
                session.runtime.drain_all();
                cx.notify();
            });
        }
    }
}
