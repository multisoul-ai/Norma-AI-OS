use crate::app::MultisoulApp;
use crate::preview::local_content::{kind, Kind, LocalPreview};
use crate::workspace::desktop::DesktopArtifactVersion;
use gpui::Context;
use std::io::Write;
use storage::{DesktopContentKind, MobileTaskDeliveryStore};
use workspace::content::Project;
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
        let id = tab.id.clone();
        let workspace_id = tab.workspace_id.clone();
        let tab_resource = tab.resource.clone();
        let expected_tab = tab.clone();
        let load_id = self.desktop_workspace.content_load_sequence.wrapping_add(1);
        self.desktop_workspace.content_load_sequence = load_id;
        self.desktop_workspace
            .content_load_versions
            .insert(id.clone(), load_id);
        let work = if let Some(encoded) = tab.version.as_deref() {
            let (Some(area_id), Ok(version)) = (
                tab.source_area_id.as_deref(),
                serde_json::from_str::<DesktopArtifactVersion>(encoded),
            ) else {
                self.desktop_workspace.content_previews.insert(
                    id,
                    Err("旧产物标签缺少版本来源，请从运行概览重新打开".into()),
                );
                cx.notify();
                return;
            };
            let scope = format!("desktop:{}:{area_id}", tab.workspace_id);
            let store = MobileTaskDeliveryStore::new(self.database_paths.app_db_path().to_owned());
            let resource = tab.resource.clone();
            cx.background_executor().spawn(async move {
                let frozen = store
                    .frozen_file_version(
                        &scope,
                        version.node_id,
                        &version.fingerprint,
                        &version.source_path,
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "任务成果版本已不可用".to_string())?;
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
                        .map_err(|error| error.to_string())
                } else {
                    Err("该二进制文件暂不支持内嵌预览".into())
                }
            })
        } else if let Some(preview_type) = tab.preview_type {
            let path = self.resolved_content_path(&tab);
            cx.background_executor()
                .spawn(async move { load_presented_file(path, preview_type) })
        } else {
            let resource = tab.resource;
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
            let path = std::path::PathBuf::from(resource);
            let path = if path.is_absolute() {
                path
            } else {
                root.as_deref()
                    .unwrap_or(std::path::Path::new(""))
                    .join(path)
            };
            cx.background_executor().spawn(async move {
                let project_root = root
                    .or_else(|| path.parent().map(std::path::Path::to_path_buf))
                    .ok_or("缺少文件来源目录")?;
                let project = Project::open(&project_root)?;
                let path = path.canonicalize().map_err(|e| e.to_string())?;
                let relative = path
                    .strip_prefix(project.root())
                    .map_err(|_| "文件不在来源项目内")?;
                let text = if matches!(
                    kind(&tab_resource),
                    Kind::Text | Kind::Markdown | Kind::Html
                ) {
                    Some(project.read_text(relative)?.text)
                } else {
                    None
                };
                if kind(&tab_resource) == Kind::Text {
                    return Ok(LoadedContent::Text(text.unwrap_or_default()));
                }
                let preview = LocalPreview::new(
                    project.root(),
                    &relative.to_string_lossy(),
                    text.as_deref(),
                )?;
                Ok(LoadedContent::Rich {
                    text,
                    preview,
                    snapshot: None,
                })
            })
        };
        self.desktop_workspace.content_loading.insert(id.clone());
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                if app.desktop_workspace.content_load_versions.get(&id) != Some(&load_id) {
                    return;
                }
                app.desktop_workspace.content_loading.remove(&id);
                if app.desktop_workspace.active_id.as_deref() != Some(workspace_id.as_str())
                    || !app.desktop_workspace.content_tabs.iter().any(|tab| {
                        tab.id == id
                            && tab.resource == expected_tab.resource
                            && tab.preview_type == expected_tab.preview_type
                            && tab.version == expected_tab.version
                    })
                {
                    return;
                }
                match result {
                    Ok(LoadedContent::Text(text)) => {
                        app.desktop_workspace.content_previews.insert(id, Ok(text));
                    }
                    Ok(LoadedContent::Rich {
                        text,
                        preview,
                        snapshot,
                    }) => {
                        if let Some(text) = text {
                            app.desktop_workspace
                                .content_previews
                                .insert(id.clone(), Ok(text));
                        }
                        if let Some(path) = snapshot {
                            app.desktop_workspace
                                .content_snapshots
                                .insert(id.clone(), path);
                        }
                        app.desktop_workspace.content_resources.insert(id, preview);
                    }
                    Err(error) => {
                        app.desktop_workspace
                            .content_previews
                            .insert(id, Err(error));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn load_presented_file(
    path: std::path::PathBuf,
    preview_type: canvas::PreviewType,
) -> Result<LoadedContent, String> {
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
