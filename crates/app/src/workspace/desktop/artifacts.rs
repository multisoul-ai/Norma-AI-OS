//! Refresh task-bound file references away from the GPUI foreground.

use super::DesktopArtifact;
use crate::app::MultisoulApp;
use gpui::Context;
use storage::{DesktopContentKind, MobileTaskDeliveryStore};

impl MultisoulApp {
    pub(crate) fn mark_visible_desktop_artifact_viewed(
        &mut self,
        tab: &storage::DesktopContentTab,
        cx: &mut Context<Self>,
    ) {
        if !self.desktop_workspace.drawers.right_open
            || !self
                .desktop_workspace
                .content_tabs
                .iter()
                .any(|current| current.id == tab.id && current.selected)
        {
            return;
        }
        if let (Some(area), Some(version)) = (
            tab.source_area_id.as_deref(),
            tab.version
                .as_deref()
                .and_then(|raw| serde_json::from_str::<super::DesktopArtifactVersion>(raw).ok()),
        ) {
            self.mark_desktop_artifacts_viewed(
                area,
                version.node_id,
                &version.fingerprint,
                Some(&version.source_path),
                cx,
            );
        }
    }

    pub(crate) fn acknowledge_workbench_review(&mut self, cx: &mut Context<Self>) {
        let s = &mut self.desktop_workspace.workbench;
        if !self.desktop_workspace.drawers.right_open
            || !self
                .desktop_workspace
                .content_tabs
                .iter()
                .any(|tab| tab.selected && tab.kind == DesktopContentKind::ProjectFiles)
            || s.project.is_none()
            || s.git.is_none()
            || s.git_error.is_some()
            || s.error.is_some()
            || s.loading
            || s.refreshing
            || s.pending_turn_review.is_none()
            || s.project_area != s.saved.area_id
        {
            return;
        }
        if let Some((_, _, _, Some(path))) = &s.pending_turn_review {
            if !s.show_diff
                || s.diff.as_deref().is_none_or(|diff| diff.trim().is_empty())
                || s.saved.selected.as_ref() != Some(path)
            {
                return;
            }
        } else if !s.turn_review_list_visible(cx) {
            return;
        }
        if let Some((area, node, fingerprint, _)) = s.pending_turn_review.take() {
            if s.saved.area_id.as_deref() == Some(area.as_str()) {
                self.mark_desktop_artifacts_viewed(&area, node, &fingerprint, None, cx);
            }
        }
    }

    /// A committed file has no live diff; show its exact frozen version instead.
    pub(crate) fn open_pending_review_artifact(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.desktop_workspace.drawers.right_open
            || !self
                .desktop_workspace
                .content_tabs
                .iter()
                .any(|tab| tab.selected && tab.kind == DesktopContentKind::ProjectFiles)
        {
            return;
        }
        let s = &mut self.desktop_workspace.workbench;
        let Some((area, node, fingerprint, Some(target))) = s.pending_turn_review.take() else {
            return;
        };
        s.pending_turn_diff = None;
        let artifact = self
            .desktop_workspace
            .content_artifacts
            .iter()
            .find(|item| {
                item.area_id == area
                    && item.node_id == node
                    && item.version == fingerprint
                    && item.source_path == path
                    && path == target
            })
            .cloned();
        if let Some(artifact) = artifact {
            self.open_desktop_artifact(artifact, cx);
        } else {
            self.desktop_workspace.error = Some(ui::i18n::LocalizedText::new(
                "drawers.file_no_longer_pending",
            ));
            cx.notify();
        }
    }

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
                    load_desktop_artifacts(&database, &background_workspace_id, &areas)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.desktop_workspace.artifact_refreshing = false;
                if app.desktop_workspace.active_id.as_deref() != Some(workspace_id.as_str()) {
                    return;
                }
                match artifacts {
                    Ok(mut artifacts) => {
                        // A refresh started before a read receipt must not resurrect that reminder.
                        for artifact in &mut artifacts {
                            artifact.viewed |=
                                app.desktop_workspace.content_artifacts.iter().any(|old| {
                                    old.viewed
                                        && old.read_key(&workspace_id)
                                            == artifact.read_key(&workspace_id)
                                });
                        }
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

    /// Read receipts are independent of delivery acceptance and retain the original artifact.
    pub(crate) fn mark_desktop_artifacts_viewed(
        &mut self,
        area_id: &str,
        node_id: u64,
        fingerprint: &str,
        path: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        let keys: Vec<_> = self
            .desktop_workspace
            .content_artifacts
            .iter()
            .filter(|item| {
                !item.viewed
                    && item.area_id == area_id
                    && item.node_id == node_id
                    && item.version == fingerprint
                    && path.is_none_or(|path| item.source_path == path)
            })
            .map(|item| (item.read_key(&workspace_id), Some("true".to_owned())))
            .collect();
        if keys.is_empty() {
            return;
        }
        let settings = storage::SettingsStore::new(self.database_paths.app_db_path().to_owned());
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move {
                    settings.apply_string_changes(&keys)?;
                    crate::mobile::project_events::changed();
                    Ok::<_, storage::StorageError>(keys)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.active_id.as_deref() != Some(workspace_id.as_str()) {
                    return;
                }
                match saved {
                    Ok(keys) => {
                        for artifact in &mut app.desktop_workspace.content_artifacts {
                            if keys
                                .iter()
                                .any(|(key, _)| *key == artifact.read_key(&workspace_id))
                            {
                                artifact.viewed = true;
                            }
                        }
                        app.mark_workspace_activity_dirty(cx);
                    }
                    Err(error) => {
                        app.desktop_workspace.error = Some(ui::i18n::message(
                            "common.operation_failed",
                            &[("reason", error.to_string())],
                        ))
                    }
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
        let generation = self.begin_desktop_content_navigation();
        let epoch = self.desktop_workspace.content_workspace_epoch;
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
                    || app.desktop_workspace.content_workspace_epoch != epoch
                    || app.desktop_workspace.content_tab_generation != generation
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

/// Shared background-only loader; receipts are scoped by workspace, area, node and version.
pub(super) fn load_desktop_artifacts(
    database: &std::path::Path,
    workspace_id: &str,
    areas: &[storage::DesktopProjectArea],
) -> storage::StorageResult<Vec<DesktopArtifact>> {
    let settings = storage::SettingsStore::new(database.to_owned());
    let store = MobileTaskDeliveryStore::new(database.to_owned());
    let mut artifacts = Vec::new();
    for area in areas {
        let source = format!("desktop:{}:{}", workspace_id, area.id);
        let refs = store.reviewable_file_refs(&source)?;
        for item in refs {
            let path = area.root.join(&item.path);
            let Ok(path) = path.canonicalize() else {
                continue;
            };
            if !path.starts_with(&area.root) || !path.is_file() {
                continue;
            }
            let artifact = DesktopArtifact {
                area_id: area.id.clone(),
                node_id: item.node_id,
                agent_profile: item.profile,
                turn_id: item.turn_id,
                path,
                source_path: item.path,
                version: item.fingerprint,
                saved_at_unix: item.saved_at_unix,
                viewed: false,
            };
            artifacts.push(artifact);
        }
    }
    artifacts.sort_by_key(|item| std::cmp::Reverse(item.saved_at_unix));
    artifacts.truncate(80);
    for artifact in &mut artifacts {
        artifact.viewed = settings
            .get_string(&artifact.read_key(workspace_id))?
            .is_some();
    }
    Ok::<_, storage::StorageError>(artifacts)
}
