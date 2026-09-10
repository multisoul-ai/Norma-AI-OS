use super::{
    conversion::node_error, execute_card_command_for_area_with_strategy, NormaCommandEffects,
    NormaExecution,
};
use crate::motion::MotionStore;
use canvas::{CanvasNodeKind, CanvasRect};
use norma::protocol::{CardCommand, CardCommandError, CardCommandResult, NodeId};
use runtime::{NodeInput, Runtime};

pub(crate) fn execute_card_command_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    command: CardCommand,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    execute_card_command_for_area_with_strategy(
        runtime,
        motion,
        master_layout,
        crate::canvas::layout_strategy::CanvasLayoutStrategy::default(),
        command,
        area,
        zoom,
    )
}

pub(crate) fn text_for_submit(mut text: String, submit: bool) -> String {
    if submit && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

pub(super) fn send_text(
    runtime: &mut Runtime,
    node_id: NodeId,
    text: String,
) -> Result<NormaExecution, CardCommandError> {
    if runtime
        .document
        .node(node_id)
        .is_some_and(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
    {
        return Err(CardCommandError::Unsupported {
            message: "Agent text must be submitted through dispatch_agent_tasks".into(),
        });
    }
    runtime
        .send_node_input(node_id, NodeInput::Text(text))
        .map_err(|error| node_error(node_id, error))?;
    Ok(NormaExecution {
        result: CardCommandResult::Sent { node_id },
        effects: NormaCommandEffects::NOTIFY,
        removed_agent_definition_id: None,
    })
}

pub(super) fn focus_card(
    runtime: &mut Runtime,
    node_id: NodeId,
) -> Result<NormaExecution, CardCommandError> {
    if runtime.document.node(node_id).is_none() {
        return Err(CardCommandError::NotFound { node_id });
    }
    if !runtime.node_caps(node_id).focusable {
        return Err(CardCommandError::Unsupported {
            message: format!("card {node_id} is not focusable"),
        });
    }
    runtime.selection.select_only(node_id);
    runtime.focus.focus_node(node_id);
    Ok(NormaExecution {
        result: CardCommandResult::Focused { node_id },
        effects: NormaCommandEffects::NOTIFY,
        removed_agent_definition_id: None,
    })
}
