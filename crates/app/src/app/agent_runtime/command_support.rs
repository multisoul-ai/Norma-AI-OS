use crate::app::MultisoulApp;

#[derive(Debug)]
pub(super) struct AgentTaskSubmitError {
    pub(super) message: String,
    retry_safe: bool,
}

impl AgentTaskSubmitError {
    pub(super) fn retry_safe(&self) -> bool {
        self.retry_safe
    }
}

impl std::fmt::Display for AgentTaskSubmitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

pub(super) fn type_agent_task(
    runtime: &mut runtime::Runtime,
    node_id: canvas::NodeId,
    task: &str,
) -> Result<(), AgentTaskSubmitError> {
    runtime
        .send_node_input(node_id, runtime::NodeInput::Paste(task.to_string()))
        .map_err(|error| AgentTaskSubmitError {
            message: error.to_string(),
            retry_safe: true,
        })
}

pub(super) fn submit_agent_enter(
    runtime: &mut runtime::Runtime,
    node_id: canvas::NodeId,
) -> Result<(), AgentTaskSubmitError> {
    if let Err(error) = runtime.send_node_input(
        node_id,
        runtime::NodeInput::Key(terminal::KeyInput {
            key: terminal::Key::Enter,
            ctrl: false,
            alt: false,
        }),
    ) {
        let cleanup = runtime.send_node_input(
            node_id,
            runtime::NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Char('u'),
                ctrl: true,
                alt: false,
            }),
        );
        return match cleanup {
            Ok(()) => Err(AgentTaskSubmitError {
                message: error.to_string(),
                retry_safe: true,
            }),
            Err(cleanup_error) => Err(AgentTaskSubmitError {
                message: format!(
                    "{error}; failed to clear unsubmitted Agent task input: {cleanup_error}"
                ),
                retry_safe: false,
            }),
        };
    }
    Ok(())
}

pub(super) fn clear_agent_task_input(
    runtime: &mut runtime::Runtime,
    node_id: canvas::NodeId,
) -> Result<(), String> {
    runtime
        .send_node_input(
            node_id,
            runtime::NodeInput::Key(terminal::KeyInput {
                key: terminal::Key::Char('u'),
                ctrl: true,
                alt: false,
            }),
        )
        .map_err(|error| error.to_string())
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
    let bounds = app.next_tiled_node_bounds_for_area(area, zoom);
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
    app.apply_layout_for_new_node_for_area(node_id, area, zoom);
    app.pending_agent_submissions.mark_starting(node_id);
    app.mark_canvas_dirty();
    app.mark_session_dirty();
    Ok((node_id, false))
}

pub(super) fn canvas_agent_profile(profile: norma::protocol::AgentProfile) -> canvas::AgentProfile {
    match profile {
        norma::protocol::AgentProfile::Claude => canvas::AgentProfile::Claude,
        norma::protocol::AgentProfile::Codex => canvas::AgentProfile::Codex,
        norma::protocol::AgentProfile::Cursor => canvas::AgentProfile::Cursor,
        norma::protocol::AgentProfile::Grok => canvas::AgentProfile::Grok,
        norma::protocol::AgentProfile::OpenCode => canvas::AgentProfile::OpenCode,
    }
}
