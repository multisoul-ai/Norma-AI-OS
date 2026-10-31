use super::{AdapterCommand, ToolContext};
use crate::AgentInputSource;

/// Model-written task text may omit background. Carry the actual transcript at
/// the typed dispatch boundary, after validation and without changing retry IDs.
pub(super) fn preserve_doubao_request(command: &mut AdapterCommand, context: &ToolContext) {
    if context.input_source != AgentInputSource::DoubaoVoice || context.user_input.trim().is_empty()
    {
        return;
    }
    let text = match command {
        AdapterCommand::AssignAgentTask { task, .. }
        | AdapterCommand::AssignWorkspaceTask { task, .. }
        | AdapterCommand::CreateAgent {
            task: Some(task), ..
        } => task,
        AdapterCommand::ReplyAgent { message, .. } => message,
        AdapterCommand::HandoffToBrain { request } => request,
        _ => return,
    };
    if text.contains(&context.user_input) {
        return;
    }
    text.push_str(
        "\n\nOriginal user request (verbatim):\n\
         Use the full request below to preserve this assignment's background, requirements, \
         examples and constraints. Do not take over work assigned to other agents.\n",
    );
    text.push_str(&context.user_input);
}
