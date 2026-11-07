//! Exact-session operations the island reuses: approvals and opening review/file results.
use crate::app::MultisoulApp;
use gpui::Context;
use norma::mobile::{RunOverviewAttention, RunOverviewAttentionKind};

impl MultisoulApp {
    pub(crate) fn with_overview_session<R>(
        &mut self,
        area_id: &str,
        cx: &mut Context<Self>,
        run: impl FnOnce(&mut MultisoulApp, &mut Context<MultisoulApp>) -> R,
    ) -> Option<R> {
        let workspace_id = self.desktop_workspace.active_id.clone()?;
        self.with_overview_session_at(&workspace_id, area_id, cx, run)
    }

    pub(crate) fn with_overview_session_at<R>(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        cx: &mut Context<Self>,
        run: impl FnOnce(&mut MultisoulApp, &mut Context<MultisoulApp>) -> R,
    ) -> Option<R> {
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))?
            .clone();
        Some(session.update(cx, run))
    }

    pub(crate) fn decide_overview_approval_at(
        &mut self,
        workspace_id: &str,
        area_id: &str,
        target: &norma::mobile::ApprovalTarget,
        allow: bool,
        cx: &mut Context<Self>,
    ) -> Result<async_channel::Receiver<Result<serde_json::Value, String>>, &'static str> {
        self.with_overview_session_at(workspace_id, area_id, cx, |app, cx| {
            if !app
                .runtime
                .mobile_approval_snapshots(target.node_id)?
                .iter()
                .any(|a| &a.target == target)
            {
                return Err("approval_changed");
            }
            app.decide_approval_by_user(
                target.node_id,
                &target.run_id,
                &target.request_id,
                allow,
                cx,
            )
        })
        .ok_or("approval_unavailable")?
    }

    /// Opens a pending review or file result exactly where the original card produced it.
    pub(in crate::workspace::desktop) fn open_attention_result(
        &mut self,
        row: &RunOverviewAttention,
        cx: &mut Context<Self>,
    ) {
        let fingerprint = row.fingerprint.clone().unwrap_or_default();
        match row.kind {
            RunOverviewAttentionKind::Review => {
                let direct_diff = row.paths.len() == 1;
                let target = direct_diff.then(|| row.paths.first().cloned()).flatten();
                self.open_turn_changes(row.area_id.clone(), row.paths.clone(), direct_diff, cx);
                self.desktop_workspace.workbench.pending_turn_review =
                    Some((row.area_id.clone(), row.node_id, fingerprint, target));
            }
            RunOverviewAttentionKind::File => {
                let path = row.paths.first().cloned().unwrap_or_default();
                self.open_overview_file(&row.area_id, row.node_id, &path, &fingerprint, cx);
            }
            _ => {}
        }
    }

    fn open_turn_changes(
        &mut self,
        area_id: String,
        paths: Vec<String>,
        direct_diff: bool,
        cx: &mut Context<Self>,
    ) {
        self.open_file_workbench(cx);
        let first = paths.first().cloned();
        let state = &mut self.desktop_workspace.workbench;
        state.generation = state.generation.wrapping_add(1);
        state.file_generation = state.file_generation.wrapping_add(1);
        state.loading = false;
        state.error = None;
        state.git_error = None;
        state.saved.selected = None;
        state.lock_area = true;
        state.saved.area_id = Some(area_id);
        state.changes_section_closed = false;
        state.turn_paths = Some(paths);
        state.pending_turn_diff = if direct_diff { first } else { None };
        state.project_area = None;
    }

    fn open_overview_file(
        &mut self,
        area_id: &str,
        node_id: u64,
        path: &str,
        fingerprint: &str,
        cx: &mut Context<Self>,
    ) {
        let artifact = self
            .desktop_workspace
            .content_artifacts
            .iter()
            .find(|item| {
                item.area_id == area_id
                    && item.node_id == node_id
                    && item.source_path == path
                    && item.version == fingerprint
            })
            .cloned();
        if let Some(artifact) = artifact {
            self.open_desktop_artifact(artifact, cx);
            return;
        }
        self.desktop_workspace.error = Some(ui::i18n::LocalizedText::new(
            "drawers.file_no_longer_pending",
        ));
        cx.notify();
    }
}
