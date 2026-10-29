//! Refresh task-bound file references away from the GPUI foreground.

use super::DesktopArtifact;
use crate::app::MultisoulApp;
use gpui::Context;
use storage::{DesktopContentKind, MobileTaskDeliveryStore};

#[cfg(test)]
#[path = "../../../tests/unit/desktop_artifact_scope_tests.rs"]
mod tests;

impl MultisoulApp {
    pub(super) fn refresh_desktop_artifacts_if_changed(&mut self, cx: &mut Context<Self>) {
        let revision = crate::mobile::project_events::revision();
        if self.desktop_workspace.artifact_refreshing
            || revision == self.desktop_workspace.artifact_revision
        {
            return;
        }
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let areas = self.desktop_workspace.areas.clone();
        if areas.is_empty() {
            return;
        }
        self.desktop_workspace.artifact_refreshing = true;
        let database = self.database_paths.app_db_path().to_path_buf();
        let background_workspace_id = workspace_id.clone();
        cx.spawn(async move |this, cx| {
            let artifacts = cx
                .background_executor()
                .spawn(async move {
                    let store = MobileTaskDeliveryStore::new(database);
                    let mut artifacts = Vec::new();
                    for area in areas {
                        let source = format!("desktop:{}:{}", background_workspace_id, area.id);
                        let refs = store.reviewable_file_refs(&source)?;
                        for item in refs {
                            let path = area.root.join(&item.path);
                            let Ok(path) = path.canonicalize() else {
                                continue;
                            };
                            if !path.starts_with(&area.root) || !path.is_file() {
                                continue;
                            }
                            artifacts.push(DesktopArtifact {
                                area_id: area.id.clone(),
                                node_id: item.node_id,
                                agent_profile: item.profile,
                                turn_id: item.turn_id,
                                path,
                                source_path: item.path,
                                version: item.fingerprint,
                                saved_at_unix: item.saved_at_unix,
                            });
                        }
                    }
                    artifacts.sort_by_key(|item| std::cmp::Reverse(item.saved_at_unix));
                    artifacts.truncate(80);
                    Ok::<_, storage::StorageError>(artifacts)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.desktop_workspace.artifact_refreshing = false;
                if app.desktop_workspace.active_id.as_deref() != Some(workspace_id.as_str()) {
                    return;
                }
                match artifacts {
                    Ok(artifacts) => {
                        app.desktop_workspace.content_artifacts = artifacts;
                        app.desktop_workspace.artifact_revision = revision;
                        app.mark_workspace_activity_dirty(cx);
                    }
                    Err(error) => tracing::warn!(%error, "desktop artifact refresh failed"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_desktop_artifact(
        &mut self,
        artifact: DesktopArtifact,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let Some(area) = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == artifact.area_id)
        else {
            self.desktop_workspace.error = Some(ui::i18n::LocalizedText::new(
                "drawers.source_project_removed",
            ));
            cx.notify();
            return;
        };
        let root = area.root.clone();
        let path = artifact.path.clone();
        cx.spawn(async move |this, cx| {
            let valid = cx
                .background_executor()
                .spawn(async move {
                    path.canonicalize()
                        .is_ok_and(|canonical| canonical.starts_with(root) && canonical.is_file())
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.active_id.as_deref() != Some(workspace_id.as_str())
                    || !app
                        .desktop_workspace
                        .areas
                        .iter()
                        .any(|area| area.id == artifact.area_id)
                {
                    return;
                }
                if !valid {
                    app.desktop_workspace.error =
                        Some(ui::i18n::LocalizedText::new("drawers.file_moved"));
                    cx.notify();
                    return;
                }
                let title = artifact
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| ui::i18n::text("drawers.files").to_string());
                app.open_desktop_content_tab_with_version(
                    DesktopContentKind::File,
                    artifact.path.to_string_lossy().into_owned(),
                    title,
                    Some(artifact.area_id),
                    Some(
                        serde_json::to_string(&super::DesktopArtifactVersion {
                            node_id: artifact.node_id,
                            fingerprint: artifact.version,
                            source_path: artifact.source_path,
                        })
                        .expect("fixed artifact reference must serialize"),
                    ),
                    cx,
                );
            });
        })
        .detach();
    }
}
