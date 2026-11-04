//! Shared browser and file previews for user navigation and assistant-opened tabs.
use super::super::content_node_id;
use crate::app::MultisoulApp;
use crate::preview::local_content::{preview_kind, Kind};
use gpui::{div, prelude::*, px, rgba, AnyElement, Context};
use storage::{DesktopContentKind, DesktopContentTab};

impl MultisoulApp {
    pub(super) fn render_content_tab_body(
        &mut self,
        tab: &DesktopContentTab,
        width: f32,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if tab.kind == DesktopContentKind::ProjectFiles {
            return self.render_file_workbench(tab, width, window, cx);
        }
        let mut content = div().size_full().flex().flex_col();
        let browser = tab.kind == DesktopContentKind::Browser;
        if browser {
            content = content.child(self.render_content_browser_toolbar(tab, width, window, cx));
        }
        let url = if tab.kind == DesktopContentKind::Browser {
            Some(tab.resource.clone())
        } else {
            self.desktop_workspace
                .content_resources
                .get(&tab.id)
                .map(|p| p.url.clone())
        };
        if let Some(url) = url {
            // A native view paints white until its first commit, so keep a themed status page
            // underneath; about:blank never needs a native view at all.
            let blank = browser && url == "about:blank";
            let theme = self.desktop_theme();
            let node_id = content_node_id(&tab.id);
            if blank {
                // A tab navigated to about:blank may still own a native view; it would cover the hint.
                self.browsers.hide(node_id);
            }
            let status = if blank {
                Some(ui::i18n::text("drawers.blank_page_hint"))
            } else {
                self.browsers
                    .awaiting_first_commit(node_id)
                    .then(|| ui::i18n::text("drawers.browser_loading"))
            };
            return content
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h(px(0.0))
                        .child(
                            div()
                                .absolute()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .children(status),
                        )
                        .when(!blank, |body| {
                            body.child(self.browser_scene_element_filling(
                                content_node_id(&tab.id),
                                url,
                                cx,
                            ))
                        }),
                )
                .into_any_element();
        }
        let text = self.desktop_workspace.content_previews.get(&tab.id);
        let message = match text {
            Some(Err(error)) => error.to_string(),
            Some(Ok(text)) if preview_kind(&tab.resource, tab.preview_type) == Kind::Text => {
                text.clone()
            }
            _ => ui::i18n::text("drawers.opening_file").to_string(),
        };
        if matches!(text, Some(Err(_))) && tab.version.is_none() {
            let path = self.resolved_content_path(tab);
            content = content.child(
                ui::component::button::Button::new("desktop-file-open-system")
                    .label(ui::i18n::text("drawers.open_system"))
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
