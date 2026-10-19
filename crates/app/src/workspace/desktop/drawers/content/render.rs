//! Shared browser and file previews for user navigation and assistant-opened tabs.
use super::super::content_node_id;
use crate::app::MultisoulApp;
use crate::preview::local_content::{kind, Kind};
use gpui::{div, prelude::*, px, AnyElement, Context};
use storage::{DesktopContentKind, DesktopContentTab};

impl MultisoulApp {
    pub(super) fn render_content_tab_body(
        &mut self,
        tab: &DesktopContentTab,
        width: f32,
        height: f32,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if tab.kind == DesktopContentKind::ProjectFiles {
            return self.render_file_workbench(tab, width, height, window, cx);
        }
        let mut content = div().size_full().flex().flex_col();
        let url = if tab.kind == DesktopContentKind::Browser {
            Some(tab.resource.clone())
        } else {
            self.desktop_workspace
                .content_resources
                .get(&tab.id)
                .map(|p| p.url.clone())
        };
        if let Some(url) = url {
            return content
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h(px(0.0))
                        .child(self.browser_scene_element(
                            content_node_id(&tab.id),
                            url,
                            crate::app::browser::BrowserViewBounds {
                                x: 0.0,
                                y: 0.0,
                                width: f64::from(width),
                                height: f64::from(height),
                                bottom_corner_radius_px: 0.0,
                            },
                            cx,
                        )),
                )
                .into_any_element();
        }
        let text = self.desktop_workspace.content_previews.get(&tab.id);
        let message = match text {
            Some(Err(error)) => error.clone(),
            Some(Ok(text)) if kind(&tab.resource) == Kind::Text => text.clone(),
            _ => "正在打开文件…".to_string(),
        };
        if matches!(text, Some(Err(_))) && tab.version.is_none() {
            let path = self.resolved_content_path(tab);
            content = content.child(
                ui::component::button::Button::new("desktop-file-open-system")
                    .label("使用系统应用打开")
                    .on_click(cx.listener(move |_, _, _, cx| {
                        if let Ok(url) = url::Url::from_file_path(&path) {
                            cx.open_url(url.as_str());
                        }
                    })),
            );
        }
        content = content.child(
            div()
                .id("desktop-content-file-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .p(px(14.0))
                .text_size(px(12.0))
                .child(message),
        );
        content.into_any_element()
    }

    pub(crate) fn resolved_content_path(&self, tab: &DesktopContentTab) -> std::path::PathBuf {
        let path = std::path::PathBuf::from(&tab.resource);
        if path.is_absolute() {
            return path;
        }
        tab.source_area_id
            .as_ref()
            .and_then(|area_id| {
                self.desktop_workspace
                    .areas
                    .iter()
                    .find(|area| &area.id == area_id)
            })
            .map(|area| area.root.join(&path))
            .unwrap_or(path)
    }
}
