//! Labels and color tokens for the right-drawer reader chrome.

use storage::{DesktopContentKind, DesktopContentTab};
use ui::icons::with_alpha;

pub(super) const READER_CHROME_HEIGHT: f32 = 52.0;
pub(super) const READER_TOOLBAR_HEIGHT: f32 = 40.0;

#[derive(Clone, Copy)]
pub(super) struct ReaderChromeColors {
    pub tab_rest: u32,
    pub tab_hover: u32,
    pub tab_pressed: u32,
    pub tab_selected: u32,
    pub tab_selected_hover: u32,
    pub tab_selected_pressed: u32,
    pub tab_rest_border: u32,
    pub tab_selected_border: u32,
    pub control_rest: u32,
    pub control_hover: u32,
    pub control_pressed: u32,
    pub control_border: u32,
}

pub(super) fn reader_chrome_colors(theme: ui::Theme) -> ReaderChromeColors {
    ReaderChromeColors {
        tab_rest: with_alpha(theme.accent_blue, 0x18),
        tab_hover: with_alpha(theme.accent_blue, 0x30),
        tab_pressed: with_alpha(theme.accent_blue, 0x48),
        tab_selected: with_alpha(theme.accent_blue, 0x38),
        tab_selected_hover: with_alpha(theme.accent_blue, 0x52),
        tab_selected_pressed: with_alpha(theme.accent_blue, 0x6a),
        tab_rest_border: with_alpha(theme.accent_blue, 0x55),
        tab_selected_border: with_alpha(theme.accent_blue, 0xd0),
        control_rest: with_alpha(theme.accent_cyan, 0x18),
        control_hover: with_alpha(theme.accent_cyan, 0x34),
        control_pressed: with_alpha(theme.accent_cyan, 0x50),
        control_border: with_alpha(theme.edge_highlight, 0x66),
    }
}

pub(super) fn content_kind_label(kind: DesktopContentKind) -> &'static str {
    match kind {
        DesktopContentKind::Browser => ui::i18n::text("drawers.web"),
        DesktopContentKind::File | DesktopContentKind::ProjectFiles => {
            ui::i18n::text("drawers.files")
        }
    }
}

pub(super) fn content_context_title(tab: &DesktopContentTab) -> String {
    let base = match tab.kind {
        DesktopContentKind::Browser => browser_context_title(&tab.resource, &tab.title),
        DesktopContentKind::ProjectFiles => ui::i18n::text("drawers.files").to_string(),
        DesktopContentKind::File => file_context_title(&tab.resource, &tab.title),
    };
    if tab.source_removed {
        ui::i18n::format("drawers.source_removed", &[("base", (base).to_string())])
    } else {
        base
    }
}

pub(super) fn is_http_resource(resource: &str) -> bool {
    url::Url::parse(resource)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
}

pub(super) fn content_icon(kind: DesktopContentKind) -> &'static str {
    match kind {
        DesktopContentKind::Browser => "icons/toolbar-browser.svg",
        DesktopContentKind::File | DesktopContentKind::ProjectFiles => "icons/file-document.svg",
    }
}

fn browser_context_title(resource: &str, title: &str) -> String {
    if resource == "about:blank" {
        return ui::i18n::text("drawers.browser").to_string();
    }
    let host = url::Url::parse(resource).ok().and_then(|url| {
        url.host_str()
            .map(|host| host.strip_prefix("www.").unwrap_or(host).to_string())
    });
    let title = title.trim();
    match (host, title.is_empty()) {
        (Some(host), false) if title != host => format!("{host} · {title}"),
        (Some(host), _) => host,
        (None, false) => title.to_string(),
        (None, true) => resource.to_string(),
    }
}

fn file_context_title(resource: &str, title: &str) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.to_string();
    }
    std::path::Path::new(resource)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(resource)
        .to_string()
}

/// Only labels owned by the workbench or blank browser are translated.
pub(super) fn content_tab_title(tab: &DesktopContentTab) -> String {
    match tab.kind {
        DesktopContentKind::ProjectFiles => ui::i18n::text("drawers.files").into(),
        DesktopContentKind::Browser if tab.resource == "about:blank" => {
            ui::i18n::text("drawers.browser").into()
        }
        _ => tab.title.clone(),
    }
}
