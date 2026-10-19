use crate::app::MultisoulApp;
use gpui::Context;
use storage::DesktopContentKind;

impl MultisoulApp {
    pub(crate) fn route_workbench_navigation(
        &mut self,
        node_id: canvas::NodeId,
        target: String,
        cx: &mut Context<Self>,
    ) {
        let current = self.desktop_workspace.content_tabs.iter().any(|t| {
            t.selected
                && t.kind == DesktopContentKind::ProjectFiles
                && super::super::super::content_node_id(&t.id) == node_id
        });
        if !current {
            self.route_file_tab_navigation(node_id, target, cx);
            return;
        }
        let Ok(url) = url::Url::parse(&target) else {
            return;
        };
        match url.scheme() {
            "http" | "https" => {
                let title = url.host_str().unwrap_or("网页").to_string();
                self.open_desktop_content_tab(
                    DesktopContentKind::Browser,
                    target,
                    title,
                    self.desktop_workspace.workbench.saved.area_id.clone(),
                    cx,
                );
            }
            "norma-content" => {
                let s = &self.desktop_workspace.workbench;
                let same_source = s
                    .preview
                    .as_ref()
                    .and_then(|p| url::Url::parse(&p.url).ok())
                    .is_some_and(|p| p.host_str() == url.host_str());
                if !same_source {
                    return;
                }
                let path = url::Url::parse(&format!("file://{}", url.path()))
                    .ok()
                    .and_then(|p| p.to_file_path().ok());
                if let Some(path) =
                    path.and_then(|p| p.strip_prefix("/").ok().map(std::path::Path::to_path_buf))
                {
                    self.select_workbench_path(
                        path.to_string_lossy().to_string(),
                        false,
                        false,
                        cx,
                    );
                }
            }
            _ => {}
        }
    }
}

impl MultisoulApp {
    fn route_file_tab_navigation(
        &mut self,
        node_id: canvas::NodeId,
        target: String,
        cx: &mut Context<Self>,
    ) {
        let tab = self
            .desktop_workspace
            .content_tabs
            .iter()
            .find(|t| {
                t.selected
                    && t.kind == DesktopContentKind::File
                    && super::super::super::content_node_id(&t.id) == node_id
            })
            .cloned();
        let Some(tab) = tab else {
            return;
        };
        let Ok(url) = url::Url::parse(&target) else {
            return;
        };
        if matches!(url.scheme(), "http" | "https") {
            let title = url.host_str().unwrap_or("网页").to_string();
            self.open_desktop_content_tab(
                DesktopContentKind::Browser,
                target,
                title,
                tab.source_area_id,
                cx,
            );
        } else if url.scheme() == "norma-content" && tab.version.is_none() {
            let same = self
                .desktop_workspace
                .content_resources
                .get(&tab.id)
                .and_then(|p| url::Url::parse(&p.url).ok())
                .is_some_and(|p| p.host_str() == url.host_str());
            if !same {
                return;
            }
            let current = self.resolved_content_path(&tab);
            let root = tab
                .source_area_id
                .as_ref()
                .and_then(|id| self.desktop_workspace.areas.iter().find(|a| &a.id == id))
                .map(|a| a.root.clone())
                .or_else(|| current.parent().map(std::path::Path::to_path_buf));
            let relative = url::Url::parse(&format!("file://{}", url.path()))
                .ok()
                .and_then(|u| u.to_file_path().ok());
            if let (Some(root), Some(relative)) = (root, relative) {
                if let Ok(relative) = relative.strip_prefix("/") {
                    let title = relative
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    self.open_desktop_content_tab(
                        DesktopContentKind::File,
                        root.join(relative).to_string_lossy().to_string(),
                        title,
                        tab.source_area_id,
                        cx,
                    );
                }
            }
        }
    }
}
