use crate::app::MultisoulApp;

impl MultisoulApp {
    pub(super) fn desktop_recent_agent_activity(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &gpui::App,
    ) -> Option<String> {
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))?;
        let app = session.read(cx);
        app.runtime.document.nodes().iter().find_map(|node| {
            let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            let Some(runtime::NodeView::Term { snapshot, .. }) = app.runtime.node_view(node.id)
            else {
                return None;
            };
            let line = snapshot
                .rows
                .iter()
                .rev()
                .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
                .map(|line| line.trim().to_string())
                .find(|line| !line.is_empty())?;
            let name = spec
                .alias
                .as_deref()
                .unwrap_or_else(|| spec.profile.display_title());
            Some(format!(
                "{}: {}",
                name,
                line.chars().take(30).collect::<String>()
            ))
        })
    }
}
