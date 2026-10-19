//! Browser controls kept independent of reader chrome and file workbench state.
use super::super::content_node_id;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, Context, Div, Stateful, Window};
use storage::{DesktopContentKind, DesktopContentTab};
use ui::component::{
    button::Button,
    input::{Input, InputState},
    Disableable, Sizable,
};

#[derive(Default)]
pub(crate) struct ContentBrowserState {
    pub input: Option<gpui::Entity<InputState>>,
    pub tab_id: Option<String>,
    pub location: Option<String>,
}

pub(super) fn browser_input(draft: &str) -> Result<String, String> {
    let draft = draft.trim();
    if draft.is_empty() {
        return Err("请输入网址或搜索内容".into());
    }
    let blocked_scheme = draft.split_once(':').is_some_and(|(scheme, _)| {
        ["javascript", "data", "file"]
            .iter()
            .any(|blocked| scheme.eq_ignore_ascii_case(blocked))
    });
    if blocked_scheme {
        return Err("浏览器支持 HTTP 和 HTTPS 地址；项目文件请从文件入口打开".into());
    }
    if draft.starts_with('/') || draft.starts_with("~/") {
        return Err("项目文件请使用文件入口打开".into());
    }
    let local = url::Url::parse(&format!("http://{draft}")).is_ok_and(|u| match u.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    });
    let query = !local
        && !draft.contains("://")
        && (draft.chars().any(char::is_whitespace) || !draft.contains(['.', ':', '/']));
    if query {
        let mut url =
            url::Url::parse("https://www.google.com/search").map_err(|e| e.to_string())?;
        url.query_pairs_mut().append_pair("q", draft);
        return Ok(url.to_string());
    }
    let candidate = if local {
        format!("http://{draft}")
    } else {
        draft.to_string()
    };
    let url = norma::presentation::normalize_browser_url(&candidate)
        .map_err(|_| "网址无效，请使用 HTTP 或 HTTPS 地址".to_string())?;
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") && url != "about:blank" {
        return Err("浏览器支持 HTTP、HTTPS 和 about:blank；项目文件请从文件入口打开".into());
    }
    Ok(url)
}

impl MultisoulApp {
    pub(super) fn open_desktop_url_from_input(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.desktop_workspace.content_browser.input.as_ref() else {
            return;
        };
        let draft = input.read(cx).value().to_string();
        let url = match browser_input(&draft) {
            Ok(url) => url,
            Err(error) => {
                self.desktop_workspace.error = Some(error.into());
                cx.notify();
                return;
            }
        };
        if let Some(tab) = self
            .desktop_workspace
            .content_tabs
            .iter()
            .find(|t| t.selected && t.kind == DesktopContentKind::Browser)
            .cloned()
        {
            let id = content_node_id(&tab.id);
            self.browsers.load_url(id, &url);
            self.update_desktop_browser_tab_location(id, url, cx);
        } else {
            let title = url::Url::parse(&url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_else(|| "浏览器".into());
            self.open_desktop_content_tab(
                DesktopContentKind::Browser,
                url,
                title,
                self.desktop_workspace.expanded_area_id.clone(),
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn render_content_tool_buttons(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .gap(px(4.0))
            .flex_none()
            .child(
                Button::new("desktop-content-files")
                    .label("文件")
                    .small()
                    .on_click(cx.listener(|app, _, _, cx| app.open_file_workbench(cx))),
            )
            .child(
                Button::new("desktop-content-browser")
                    .label("+")
                    .tooltip("新建网页")
                    .small()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.open_desktop_content_tab(
                            DesktopContentKind::Browser,
                            "about:blank".into(),
                            "浏览器".into(),
                            None,
                            cx,
                        )
                    })),
            )
    }

    pub(super) fn render_content_browser_toolbar(
        &mut self,
        tab: &DesktopContentTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if self.desktop_workspace.content_browser.tab_id.as_deref() != Some(&tab.id)
            || self.desktop_workspace.content_browser.input.is_none()
        {
            self.desktop_workspace.content_browser.input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("输入网址或搜索…")
                    .default_value(tab.resource.clone())
            }));
            self.desktop_workspace.content_browser.tab_id = Some(tab.id.clone());
        }
        let input = self
            .desktop_workspace
            .content_browser
            .input
            .as_ref()
            .expect("address initialized")
            .clone();
        if self.desktop_workspace.content_browser.location.as_ref() != Some(&tab.resource) {
            input.update(cx, |i, cx| i.set_value(tab.resource.clone(), window, cx));
            self.desktop_workspace.content_browser.location = Some(tab.resource.clone());
        }
        let empty = input.read(cx).value().trim().is_empty();
        let id = content_node_id(&tab.id);
        div()
            .id("desktop-content-browser-navigation")
            .debug_selector(|| "desktop-content-browser-navigation".into())
            .h(px(40.0))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(6.0))
            .child(self.render_content_tool_buttons(cx))
            .child(
                Button::new("desktop-browser-back")
                    .label("‹")
                    .tooltip("后退")
                    .small()
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.browsers.go_back(id);
                        cx.notify();
                    })),
            )
            .child(
                Button::new("desktop-browser-forward")
                    .label("›")
                    .tooltip("前进")
                    .small()
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.browsers.go_forward(id);
                        cx.notify();
                    })),
            )
            .child(
                Button::new("desktop-browser-reload")
                    .label("↻")
                    .tooltip("刷新")
                    .small()
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.browsers.reload(id);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("desktop-content-address-bar")
                    .debug_selector(|| "desktop-content-address-bar".into())
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(28.0))
                    .child(Input::new(&input).appearance(false))
                    .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" {
                            app.open_desktop_url_from_input(cx);
                            cx.stop_propagation();
                        }
                    })),
            )
            .child(
                Button::new("desktop-browser-navigate")
                    .label("打开")
                    .small()
                    .disabled(empty)
                    .on_click(cx.listener(|app, _, _, cx| app.open_desktop_url_from_input(cx))),
            )
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/browser_input_tests.rs"]
mod tests;
