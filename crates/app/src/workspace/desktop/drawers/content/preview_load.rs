use crate::app::MultisoulApp;
use crate::preview::content_copy::content_error;
use crate::preview::local_content::{kind, Kind, LocalPreview};
use crate::workspace::desktop::{DesktopArtifactVersion, DesktopWorkspaceUiState};
use gpui::Context;
use std::{io::Write, path::Path};
use storage::{DesktopContentKind, DesktopContentTab, MobileTaskDeliveryStore};
use ui::i18n::LocalizedText;
use workspace::content::{Comparison, Project, TextFile};
use workspace::review::ContentKind;

enum LoadedContent {
    Text(String),
    Rich {
        text: Option<String>,
        preview: LocalPreview,
        snapshot: Option<tempfile::TempPath>,
    },
}

impl MultisoulApp {
    pub(crate) fn load_selected_desktop_content(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self
            .desktop_workspace
            .content_tabs
            .iter()
            .find(|tab| tab.selected)
            .cloned()
        else {
            return;
        };
        if self
            .desktop_workspace
            .content_previews
            .get(&tab.id)
            .is_some_and(Result::is_ok)
            || self
                .desktop_workspace
                .content_resources
                .contains_key(&tab.id)
        {
            self.mark_visible_desktop_artifact_viewed(&tab, cx);
        }
        if tab.kind != DesktopContentKind::File
            || self
                .desktop_workspace
                .content_previews
                .contains_key(&tab.id)
            || self
                .desktop_workspace
                .content_snapshots
                .contains_key(&tab.id)
            || self
                .desktop_workspace
                .content_resources
                .contains_key(&tab.id)
            || self.desktop_workspace.content_loading.contains(&tab.id)
        {
            return;
        }
        self.load_desktop_file_tab(tab, false, cx);
    }

    pub(super) fn load_desktop_file_tab(
        &mut self,
        tab: DesktopContentTab,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        let id = tab.id.clone();
        let epoch = self.desktop_workspace.content_workspace_epoch;
        let navigation_generation = self.desktop_workspace.content_tab_generation;
        let expected_tab = tab.clone();
        let load_id = self.desktop_workspace.content_load_sequence.wrapping_add(1);
        self.desktop_workspace.content_load_sequence = load_id;
        self.desktop_workspace
            .content_load_versions
            .insert(id.clone(), load_id);
        let work = if let Some(encoded) = tab.version {
            let store = MobileTaskDeliveryStore::new(self.database_paths.app_db_path().to_owned());
            cx.background_executor().spawn(async move {
                let (Some(area_id), Ok(version)) = (
                    tab.source_area_id.as_deref(),
                    serde_json::from_str::<DesktopArtifactVersion>(&encoded),
                ) else {
                    return Err(LocalizedText::new("drawers.legacy_artifact"));
                };
                let scope = format!("desktop:{}:{area_id}", tab.workspace_id);
                let resource = tab.resource;
                let frozen = store
                    .frozen_file_version(
                        &scope,
                        version.node_id,
                        &version.fingerprint,
                        &version.source_path,
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| ui::i18n::LocalizedText::new("drawers.artifact_unavailable"))?;
                if kind(&resource) != Kind::Text {
                    let suffix = std::path::Path::new(&resource)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .filter(|extension| {
                            extension.len() <= 8
                                && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
                        })
                        .map(|extension| format!(".{extension}"))
                        .unwrap_or_default();
                    let mut file = tempfile::Builder::new()
                        .prefix("norma-result-")
                        .suffix(&suffix)
                        .tempfile()
                        .map_err(|error| error.to_string())?;
                    file.write_all(&frozen.bytes)
                        .map_err(|error| error.to_string())?;
                    let path = file.into_temp_path();
                    let text = if matches!(kind(&resource), Kind::Markdown | Kind::Html) {
                        Some(String::from_utf8(frozen.bytes).map_err(|e| e.to_string())?)
                    } else {
                        None
                    };
                    let preview = LocalPreview::snapshot(&path, text.as_deref())?;
                    Ok(LoadedContent::Rich {
                        text,
                        preview,
                        snapshot: Some(path),
                    })
                } else if frozen.kind == ContentKind::Text {
                    String::from_utf8(frozen.bytes)
                        .map(LoadedContent::Text)
                        .map_err(|error| error.to_string().into())
                } else {
                    Err(ui::i18n::LocalizedText::new("drawers.binary_preview"))
                }
            })
        } else if let Some(preview_type) = tab.preview_type {
            let path = self.resolved_content_path(&tab);
            cx.background_executor()
                .spawn(async move { load_presented_file(path, preview_type) })
        } else {
            let root = tab
                .source_area_id
                .as_ref()
                .and_then(|area_id| {
                    self.desktop_workspace
                        .areas
                        .iter()
                        .find(|area| &area.id == area_id)
                })
                .map(|area| area.root.clone());
            let tab_resource = tab.resource;
            let path = std::path::PathBuf::from(&tab_resource);
            let path = if path.is_absolute() {
                path
            } else {
                root.as_deref()
                    .unwrap_or(std::path::Path::new(""))
                    .join(path)
            };
            cx.background_executor()
                .spawn(async move { load_local_file(root, path, &tab_resource) })
        };
        self.desktop_workspace.content_loading.insert(id.clone());
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                let state = &mut app.desktop_workspace;
                if !file_load_is_current(state, &expected_tab, epoch, load_id) {
                    return;
                }
                state.content_loading.remove(&id);
                let installed = install_file_load(state, &id, result);
                if refresh {
                    state.content_refresh.finish(&id, installed);
                }
                if installed && state.content_tab_generation == navigation_generation {
                    app.mark_visible_desktop_artifact_viewed(&expected_tab, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn file_load_is_current(
    state: &DesktopWorkspaceUiState,
    expected: &DesktopContentTab,
    epoch: u64,
    load_id: u64,
) -> bool {
    state.content_workspace_epoch == epoch
        && state.active_id.as_ref() == Some(&expected.workspace_id)
        && state.content_load_versions.get(&expected.id) == Some(&load_id)
        && state.content_tabs.iter().any(|tab| {
            tab.id == expected.id
                && tab.instance_id == expected.instance_id
                && tab.workspace_id == expected.workspace_id
                && tab.kind == DesktopContentKind::File
                && tab.resource == expected.resource
                && tab.source_area_id == expected.source_area_id
                && tab.preview_type == expected.preview_type
                && tab.version == expected.version
        })
}

fn install_file_load(
    state: &mut DesktopWorkspaceUiState,
    id: &str,
    result: Result<LoadedContent, LocalizedText>,
) -> bool {
    let loaded = match result {
        Ok(loaded) => loaded,
        Err(error) => {
            if !state.content_resources.contains_key(id)
                && !state.content_previews.get(id).is_some_and(Result::is_ok)
            {
                state.content_previews.insert(id.to_owned(), Err(error));
            }
            return false;
        }
    };
    state.content_resources.remove(id);
    state.content_snapshots.remove(id);
    state.content_previews.remove(id);
    match loaded {
        LoadedContent::Text(text) => {
            state.content_previews.insert(id.to_owned(), Ok(text));
        }
        LoadedContent::Rich {
            text,
            preview,
            snapshot,
        } => {
            if let Some(text) = text {
                state.content_previews.insert(id.to_owned(), Ok(text));
            }
            if let Some(path) = snapshot {
                state.content_snapshots.insert(id.to_owned(), path);
            }
            state.content_resources.insert(id.to_owned(), preview);
        }
    }
    true
}

pub(super) fn is_workbench_text(path: &str) -> bool {
    matches!(kind(path), Kind::Text | Kind::Markdown | Kind::Html)
}

type WorkbenchDocument = (Option<String>, Option<TextFile>, Option<LocalPreview>);

pub(super) fn read_workbench_document(
    project: &Project,
    comparison: &Comparison,
    path: &str,
    diff: bool,
    staged: bool,
) -> Result<WorkbenchDocument, LocalizedText> {
    if diff {
        let diff = project
            .git_diff(comparison, path, staged)
            .map_err(content_error)?;
        return Ok((Some(diff), None, None));
    }
    let text = is_workbench_text(path)
        .then(|| project.read_text(Path::new(path)).map_err(content_error))
        .transpose()?;
    let preview = (kind(path) != Kind::Text)
        .then(|| LocalPreview::new(project.root(), path, text.as_ref().map(|t| t.text.as_str())))
        .transpose()?;
    Ok((None, text, preview))
}

fn load_local_file(
    root: Option<std::path::PathBuf>,
    path: std::path::PathBuf,
    tab_resource: &str,
) -> Result<LoadedContent, ui::i18n::LocalizedText> {
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    if !path
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err(content_error(
            workspace::content::ContentError::NotRegularFile,
        ));
    }
    // A file tab authorizes its selected file, not the entire external parent directory.
    // Keep project-relative assets available only when the target is inside that project.
    let project_root = root
        .and_then(|root| root.canonicalize().ok())
        .filter(|root| path.starts_with(root));
    let isolated = project_root.is_none();
    let root = project_root
        .or_else(|| path.parent().map(std::path::Path::to_path_buf))
        .ok_or(ui::i18n::LocalizedText::new("drawers.missing_file_source"))?;
    let project = Project::open(&root).map_err(content_error)?;
    let relative = path
        .strip_prefix(project.root())
        .map_err(|_| ui::i18n::LocalizedText::new("drawers.outside_project"))?;
    let text = if matches!(kind(tab_resource), Kind::Text | Kind::Markdown | Kind::Html) {
        Some(project.read_text(relative).map_err(content_error)?.text)
    } else {
        None
    };
    if kind(tab_resource) == Kind::Text {
        return Ok(LoadedContent::Text(text.unwrap_or_default()));
    }
    let preview = if isolated {
        LocalPreview::snapshot(&path, text.as_deref())?
    } else {
        LocalPreview::new(project.root(), &relative.to_string_lossy(), text.as_deref())?
    };
    Ok(LoadedContent::Rich {
        text,
        preview,
        snapshot: None,
    })
}

fn load_presented_file(
    path: std::path::PathBuf,
    preview_type: canvas::PreviewType,
) -> Result<LoadedContent, ui::i18n::LocalizedText> {
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    let text = if preview_type != canvas::PreviewType::Image {
        // Reuse Preview's bounded, UTF-8 checked reader for the explicitly presented file.
        crate::preview::load_preview_content(path.clone(), Some(preview_type))
            .map_err(|error| format!("{error:?}"))?
            .text_source()
            .map(str::to_owned)
    } else {
        None
    };
    if preview_type == canvas::PreviewType::Text {
        return Ok(LoadedContent::Text(text.unwrap_or_default()));
    }
    let preview = LocalPreview::presented(&path, text.as_deref(), preview_type)?;
    Ok(LoadedContent::Rich {
        text,
        preview,
        snapshot: None,
    })
}

#[cfg(test)]
#[path = "../../../../../tests/unit/desktop_presentation_preview_tests.rs"]
mod presentation_preview_tests;

#[cfg(test)]
#[path = "../../../../../tests/unit/desktop_external_file_tests.rs"]
mod external_file_tests;
