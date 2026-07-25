use crate::app::MultisoulApp;
use norma::protocol::{AgentProfile, CardCommandResult};

pub(super) fn create_or_reuse_agent(
    app: &mut MultisoulApp,
    name: Option<String>,
    profile: AgentProfile,
    activate: bool,
    area: canvas::CanvasRect,
    zoom: f32,
) -> Result<CardCommandResult, String> {
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
                return Err(format!(
                    "{} 画布上已有卡片，请直接对话，不要再创建",
                    existing.name
                ));
            }
            let node_id = if activate {
                Some(activate_definition(app, &existing, area, zoom)?.0)
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
        .create_definition(name, canvas_agent_profile(profile))?;
    let node_id = if activate {
        Some(activate_definition(app, &definition, area, zoom)?.0)
    } else {
        None
    };
    Ok(CardCommandResult::AgentCreated {
        definition_id: definition.id,
        name: definition.name,
        node_id,
    })
}

pub(super) fn activate_definition(
    app: &mut MultisoulApp,
    definition: &storage::AgentDefinition,
    area: canvas::CanvasRect,
    zoom: f32,
) -> Result<(u64, bool), String> {
    if let Some(node_id) = app.runtime.document.nodes().iter().find_map(|node| {
        let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
            return None;
        };
        (spec.definition_id == Some(definition.id)).then_some(node.id)
    }) {
        return Ok((node_id, true));
    }
    let bounds = area.inset(app.layout_gap_canvas());
    let (cols, rows) = crate::canvas::master_layout::term_grid_size_for_card(
        canvas::NodeType::Agent,
        bounds,
        zoom,
    );
    let node_id = app
        .runtime
        .spawn_agent_for_definition(
            definition.runtime_profile,
            definition.id,
            definition.name.clone(),
            bounds,
            cols,
            rows,
        )
        .map_err(|error| error.to_string())?;
    crate::canvas::spawn_focus::activate_spawned_node(&mut app.runtime, node_id);
    app.motion.spawn_node(node_id, bounds);
    app.motion.start_initialization(
        node_id,
        crate::motion::InitializationAnimationKind::for_agent_profile(definition.runtime_profile),
    );
    app.pending_agent_submissions.mark_starting(node_id);
    app.mark_canvas_dirty();
    app.mark_session_dirty();
    Ok((node_id, false))
}

pub(super) fn canvas_agent_profile(profile: AgentProfile) -> canvas::AgentProfile {
    match profile {
        AgentProfile::Claude => canvas::AgentProfile::Claude,
        AgentProfile::Codex => canvas::AgentProfile::Codex,
        AgentProfile::Cursor => canvas::AgentProfile::Cursor,
        AgentProfile::Grok => canvas::AgentProfile::Grok,
        AgentProfile::OpenCode => canvas::AgentProfile::OpenCode,
    }
}
