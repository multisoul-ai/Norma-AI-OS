use crate::app::MultisoulApp;
use norma::protocol::{AgentProfile, CardCommandError, CardCommandResult};

use super::command_protocol::internal_agent_error;
use super::command_support::{activate_definition, canvas_agent_profile};

pub(super) fn create_or_reuse_agent(
    app: &mut MultisoulApp,
    name: Option<String>,
    profile: AgentProfile,
    activate: bool,
    area: canvas::CanvasRect,
    zoom: f32,
) -> Result<CardCommandResult, CardCommandError> {
    if let Some(requested_name) = name.as_deref() {
        if let Some(existing) = app
            .agent_definitions
            .definition_by_address(requested_name)
            .cloned()
        {
            let has_card = app.runtime.document.nodes().iter().any(|node| {
                matches!(
                    &node.kind,
                    canvas::CanvasNodeKind::Agent(spec)
                        if spec.definition_id == Some(existing.id)
                )
            });
            if has_card {
                return Err(internal_agent_error(format!(
                    "{} 画布上已有卡片，请直接对话，不要再创建",
                    existing.name
                )));
            }
            let node_id = if activate {
                Some(
                    activate_definition(app, &existing, area, zoom)
                        .map_err(internal_agent_error)?
                        .0,
                )
            } else {
                None
            };
            return Ok(CardCommandResult::AgentCreated {
                definition_id: existing.id,
                name: existing.name,
                node_id,
            });
        }
    }
    let definition = app
        .agent_definitions
        .create_definition(name, canvas_agent_profile(profile))
        .map_err(internal_agent_error)?;
    let node_id = if activate {
        Some(
            activate_definition(app, &definition, area, zoom)
                .map_err(internal_agent_error)?
                .0,
        )
    } else {
        None
    };
    Ok(CardCommandResult::AgentCreated {
        definition_id: definition.id,
        name: definition.name,
        node_id,
    })
}
