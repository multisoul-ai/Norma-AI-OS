//! Desktop navigation adapters for the global island.
use crate::app::MultisoulApp;
use gpui::Context;

impl MultisoulApp {
    pub(crate) fn island_navigation_ready(&self, workspace: &str, area: &str) -> bool {
        let state = &self.desktop_workspace;
        state.active_id.as_deref() == Some(workspace)
            && !state.busy
            && state.transition.is_none()
            && !state
                .loading_area_ids
                .contains(&(workspace.into(), area.into()))
    }

    pub(crate) fn open_island_artifact(
        &mut self,
        workspace: &str,
        row: &norma::mobile::RunOverviewAttention,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.desktop_workspace.active_id.as_deref() != Some(workspace)
            || !matches!(
                row.kind,
                norma::mobile::RunOverviewAttentionKind::File
                    | norma::mobile::RunOverviewAttentionKind::Review
            )
            || row.paths.is_empty()
            || !row.paths.iter().all(|path| {
                self.desktop_workspace.content_artifacts.iter().any(|item| {
                    item.area_id == row.area_id
                        && item.node_id == row.node_id
                        && item.turn_id == row.turn_id
                        && Some(item.version.as_str()) == row.fingerprint.as_deref()
                        && item.source_path == *path
                })
            })
        {
            return false;
        }
        self.activate_overview_action(super::overview_rows::attention_row(row).action, window, cx);
        true
    }
}
