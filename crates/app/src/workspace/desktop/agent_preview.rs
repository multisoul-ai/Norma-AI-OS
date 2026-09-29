use crate::app::MultisoulApp;

pub(super) struct DesktopAgentSummary {
    pub(super) node_id: canvas::NodeId,
    pub(super) name: String,
    pub(super) status: &'static str,
    pub(super) location: String,
    pub(super) last_line: String,
}

impl MultisoulApp {
    pub(super) fn desktop_agent_summaries(
        &self,
        workspace_id: &str,
        area_id: &str,
        cx: &gpui::App,
    ) -> Vec<DesktopAgentSummary> {
        let Some(session) = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))
        else {
            return Vec::new();
        };
        let app = session.read(cx);
        app.runtime
            .document
            .nodes()
            .iter()
            .filter_map(|node| {
                let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                let (status, last_line) = match app.runtime.node_view(node.id) {
                    Some(runtime::NodeView::Term {
                        snapshot, status, ..
                    }) => {
                        let status = if matches!(status, terminal::TerminalStatus::Running) {
                            "Running"
                        } else {
                            "Stopped"
                        };
                        let line = snapshot
                            .rows
                            .iter()
                            .rev()
                            .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
                            .map(|line| line.trim().to_string())
                            .find(|line| !line.is_empty())
                            .unwrap_or_default();
                        (status, line.chars().take(90).collect())
                    }
                    None => ("Stopped", String::new()),
                };
                Some(DesktopAgentSummary {
                    node_id: node.id,
                    name: spec
                        .alias
                        .clone()
                        .unwrap_or_else(|| spec.profile.display_title().to_string()),
                    status,
                    location: format!(
                        "{} / {}",
                        app.runtime
                            .workspace_root()
                            .and_then(|path| path.file_name())
                            .and_then(|name| name.to_str())
                            .unwrap_or("Project"),
                        app.runtime
                            .agent_worktree(node.id)
                            .and_then(|path| path.file_name())
                            .and_then(|name| name.to_str())
                            .unwrap_or("Worktree")
                    ),
                    last_line,
                })
            })
            .collect()
    }
}
