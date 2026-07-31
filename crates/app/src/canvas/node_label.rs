//! Canvas node-type labels used by the shared card title bar.

use canvas::NodeType;

pub(super) fn node_type_label(node_type: NodeType) -> &'static str {
    match node_type {
        NodeType::Term => "TERMINAL",
        NodeType::Agent => "AGENT",
        NodeType::Preview => "PREVIEW",
        NodeType::Browser => "BROWSER",
        NodeType::Rect => "NODE",
    }
}
