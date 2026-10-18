use super::{is_image_file, is_web_file};
use crate::app::MultisoulApp;
use crate::workspace::desktop::DesktopArtifactVersion;
use gpui::Context;
use std::io::Write;
use storage::{DesktopContentKind, MobileTaskDeliveryStore};
use workspace::review::ContentKind;

enum LoadedContent {
    Text(String),
    Snapshot(tempfile::TempPath),
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
            || self.desktop_workspace.content_loading.contains(&tab.id)
        {
            return;
        }
        if tab.version.is_none() && (is_image_file(&tab.resource) || is_web_file(&tab.resource)) {
            return;
        }
        let id = tab.id.clone();
        let workspace_id = tab.workspace_id.clone();
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
                if is_image_file(&resource) || is_web_file(&resource) {
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
                    Ok(LoadedContent::Snapshot(file.into_temp_path()))
                } else if frozen.kind == ContentKind::Text {
                    String::from_utf8(frozen.bytes)
                        .map(LoadedContent::Text)
                        .map_err(|error| error.to_string())
                } else {
                    Err("该二进制文件暂不支持内嵌预览".into())
                }
            })
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
                root.unwrap_or_default().join(path)
            };
            cx.background_executor().spawn(async move {
                crate::preview::load_preview_content(path, None)
                    .map(|content| {
                        LoadedContent::Text(
                            content
                                .text_source()
                                .map(ToOwned::to_owned)
                                .unwrap_or_default(),
                        )
                    })
                    .map_err(|error| error.to_string())
            })
        };
        self.desktop_workspace.content_loading.insert(id.clone());
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.desktop_workspace.content_loading.remove(&id);
                if app.desktop_workspace.active_id.as_deref() != Some(workspace_id.as_str())
                    || !app
                        .desktop_workspace
                        .content_tabs
                        .iter()
                        .any(|tab| tab.id == id)
                {
                    return;
                }
                match result {
                    Ok(LoadedContent::Text(text)) => {
                        app.desktop_workspace.content_previews.insert(id, Ok(text));
                    }
                    Ok(LoadedContent::Snapshot(path)) => {
                        app.desktop_workspace.content_snapshots.insert(id, path);
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
