//! Canvas node-type labels used by the shared card title bar.

use canvas::NodeType;

pub(super) fn node_type_label(node_type: NodeType) -> &'static str {
    match node_type {
        NodeType::Term => ui::i18n::text("canvas.type_terminal"),
        NodeType::Agent => ui::i18n::text("canvas.type_agent"),
        NodeType::Preview => ui::i18n::text("canvas.type_preview"),
        NodeType::Browser => ui::i18n::text("canvas.type_browser"),
        NodeType::Rect => ui::i18n::text("canvas.type_node"),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/canvas_node_language_tests.rs"]
mod tests;
