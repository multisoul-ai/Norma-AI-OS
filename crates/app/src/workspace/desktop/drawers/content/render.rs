//! Render selected browser and file content inside the right drawer.

use super::super::content_node_id;
use super::{is_image_file, is_web_file, GLASS_TEXT_SECONDARY};
use crate::app::MultisoulApp;
use gpui::{div, img, prelude::*, px, rgba, AnyElement, Context, ObjectFit};
use storage::{DesktopContentKind, DesktopContentTab};

impl MultisoulApp {
    pub(super) fn render_content_tab_body(
        &self,
        tab: &DesktopContentTab,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = if tab.source_removed {
            format!("{} · 来源项目已移除", tab.source_project_name)
        } else {
            tab.source_project_name.clone()
        };
        let mut content = div().size_full().flex().flex_col().child(
            div()
                .px(px(12.0))
                .py(px(9.0))
                .text_size(px(11.0))
                .text_color(rgba(GLASS_TEXT_SECONDARY))
                .child(label),
        );
        let resource = tab.resource.clone();
        let snapshot = self
            .desktop_workspace
            .content_snapshots
            .get(&tab.id)
            .map(|path| path.to_path_buf());
        if tab.version.is_some()
            && (is_web_file(&resource) || is_image_file(&resource))
            && snapshot.is_none()
        {
            let message = self
                .desktop_workspace
                .content_previews
                .get(&tab.id)
                .and_then(|result| result.as_ref().err())
                .map(String::as_str)
                .unwrap_or("正在打开任务成果版本…");
            return content
                .child(
                    div()
                        .p(px(14.0))
                        .text_size(px(12.0))
                        .child(message.to_owned()),
                )
                .into_any_element();
        }
        let path = snapshot.unwrap_or_else(|| self.resolved_content_path(tab));
        if tab.kind == DesktopContentKind::Browser || is_web_file(&resource) {
            let url = if tab.kind == DesktopContentKind::Browser {
                resource
            } else {
                url::Url::from_file_path(&path)
                    .ok()
                    .map(|url| url.to_string())
                    .unwrap_or_default()
            };
            if !url.is_empty() {
                content = content.child(div().relative().flex_1().min_h(px(0.0)).child(
                    self.browser_scene_element(
                        content_node_id(&tab.id),
                        url,
                        crate::app::browser::BrowserViewBounds {
                            x: 0.0,
                            y: 0.0,
                            width: f64::from(width - 26.0),
                            height: f64::from(height),
                            bottom_corner_radius_px: 0.0,
                        },
                        cx,
                    ),
                ));
            }
        } else if is_image_file(&tab.resource) {
            content = content.child(
                img(path)
                    .flex_1()
                    .min_h(px(0.0))
                    .object_fit(ObjectFit::Contain),
            );
        } else {
            let text = self.desktop_workspace.content_previews.get(&tab.id);
            content = content.child(
                div()
                    .id("desktop-content-file-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(px(14.0))
                    .text_size(px(12.0))
                    .child(match text {
                        Some(Ok(text)) => text.clone(),
                        Some(Err(error)) => error.clone(),
                        None => "正在打开文件…".to_string(),
                    }),
            );
        }
        content.into_any_element()
    }

    fn resolved_content_path(&self, tab: &DesktopContentTab) -> std::path::PathBuf {
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
