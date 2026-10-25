//! Membership policy shared by rendering, hit testing and layout.
use canvas::NodeType;

pub(crate) fn embedded_node_visible(node_type: NodeType, embedded_desktop: bool) -> bool {
    !embedded_desktop || matches!(node_type, NodeType::Agent | NodeType::Term)
}
