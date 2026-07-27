use canvas::{AgentProfile as CanvasAgentProfile, CanvasRect, PreviewType as CanvasPreviewType};
use norma::protocol::{
    AgentProfile, CardBounds, CardCommandError, NodeId, PreviewType as NormaPreviewType,
};
use runtime::NodeError;

pub(super) fn norma_agent_profile(value: CanvasAgentProfile) -> AgentProfile {
    match value {
        CanvasAgentProfile::Claude => AgentProfile::Claude,
        CanvasAgentProfile::Codex => AgentProfile::Codex,
        CanvasAgentProfile::Cursor => AgentProfile::Cursor,
        CanvasAgentProfile::Grok => AgentProfile::Grok,
        CanvasAgentProfile::OpenCode => AgentProfile::OpenCode,
    }
}

pub(crate) fn canvas_preview_type(value: NormaPreviewType) -> CanvasPreviewType {
    match value {
        NormaPreviewType::Markdown => CanvasPreviewType::Markdown,
        NormaPreviewType::Text => CanvasPreviewType::Text,
        NormaPreviewType::Image => CanvasPreviewType::Image,
    }
}

pub(super) fn card_bounds_to_canvas(value: CardBounds) -> CanvasRect {
    CanvasRect::new(value.x, value.y, value.width, value.height)
}

pub(super) fn node_error(node_id: NodeId, error: NodeError) -> CardCommandError {
    match error {
        NodeError::MissingNode => CardCommandError::NotFound { node_id },
        other => CardCommandError::Runtime {
            message: other.to_string(),
        },
    }
}
