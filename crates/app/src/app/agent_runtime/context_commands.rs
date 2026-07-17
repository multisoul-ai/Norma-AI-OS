use agent::{AgentCommand, AgentRosterEntry, CurrentCanvasContext};

pub(super) fn contextualized_agent_commands(
    roster: Vec<AgentRosterEntry>,
    current_canvas_context: Option<CurrentCanvasContext>,
    command: AgentCommand,
) -> Vec<AgentCommand> {
    let mut commands = Vec::with_capacity(3);
    commands.push(AgentCommand::UpdateAgentRoster(roster));
    if let Some(context) = current_canvas_context {
        commands.push(AgentCommand::UpdateCurrentCanvasContext(context));
    }
    commands.push(command);
    commands
}
