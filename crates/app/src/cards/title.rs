use canvas::{BrowserSpec, BrowserSurfaceKind, NodeKind};

/// Translate product defaults by structure; user titles and filenames are data.
pub(super) fn display_title(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Rect(_) => ui::i18n::text("cards.default_node_title").into(),
        NodeKind::Preview(spec) => std::path::Path::new(&spec.path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| ui::i18n::text("cards.default_preview_title"))
            .to_string(),
        NodeKind::Browser(spec) if matches!(spec.surface, BrowserSurfaceKind::Native) => {
            browser_title(spec)
        }
        _ => kind.display_title(),
    }
}

fn browser_title(spec: &BrowserSpec) -> String {
    if let Some(title) = spec
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
    {
        return title.to_string();
    }
    let host = spec.url.split_once("://").map(|(_, rest)| {
        rest.split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .split('@')
            .next_back()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
    });
    host.filter(|host| !host.is_empty() && !host.chars().any(char::is_whitespace))
        .unwrap_or_else(|| ui::i18n::text("cards.default_browser_title"))
        .to_string()
}
