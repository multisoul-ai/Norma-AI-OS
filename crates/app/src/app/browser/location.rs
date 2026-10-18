//! Browser card address and title updates after native navigation.

use super::MultisoulApp;
use canvas::{CanvasNodeKind, NodeId};

impl MultisoulApp {
    pub(super) fn set_browser_url(&mut self, node_id: NodeId, url: String) -> bool {
        let Some(node) = self.runtime.document.node_mut(node_id) else {
            return false;
        };
        let CanvasNodeKind::Browser(spec) = &mut node.kind else {
            return false;
        };
        let url = canonical_browser_location(&url);
        if spec.url == url && spec.title.is_none() {
            return false;
        }
        spec.url = url;
        spec.title = None;
        true
    }

    pub(super) fn set_browser_title(&mut self, node_id: NodeId, title: String) -> bool {
        let Some(node) = self.runtime.document.node_mut(node_id) else {
            return false;
        };
        let CanvasNodeKind::Browser(spec) = &mut node.kind else {
            return false;
        };
        let clean = title.trim().to_string();
        if clean.is_empty() || spec.title.as_deref() == Some(clean.as_str()) {
            return false;
        }
        spec.title = Some(clean);
        self.mark_canvas_dirty();
        true
    }
}

pub(crate) fn canonical_browser_location(input: &str) -> String {
    let trimmed = input.trim();
    url::Url::parse(trimmed)
        .map(|url| url.to_string())
        .unwrap_or_else(|_| trimmed.to_string())
}
