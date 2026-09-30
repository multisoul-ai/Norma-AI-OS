use super::{MobileCommand, MobileCommandRequest};

pub(super) fn validate(request: &MobileCommandRequest) -> Result<(), &'static str> {
    request.command.validate_feedback_target()?;
    if !valid_id(&request.operation_id)
        || !valid_id(&request.workspace_id)
        || !valid_id(&request.route_id)
        || request.grant_revision == 0
    {
        return Err("invalid_request");
    }
    match &request.command {
        MobileCommand::TerminalInput {
            expected, input, ..
        } if !valid_target(expected) || expected.run_id.is_none() || !input.valid() => {
            Err("invalid_request")
        }
        MobileCommand::AcceptDelivery { key, fingerprint }
            if key.validate().is_err()
                || fingerprint.len() != 64
                || !fingerprint.bytes().all(|c| c.is_ascii_hexdigit()) =>
        {
            Err("invalid_request")
        }
        MobileCommand::DecidePlanTodo {
            retry_of: Some(previous),
            choice,
            ..
        } if !valid_id(&previous.device_id)
            || !valid_id(&previous.operation_id)
            || !matches!(choice, agent::PlanTodoChoice::Accept { .. }) =>
        {
            Err("invalid_request")
        }
        MobileCommand::DecidePlanTodo {
            plan_id,
            todo_index,
            ..
        } if !valid_id(plan_id) || *todo_index >= 8 => Err("invalid_request"),
        MobileCommand::DecidePlanTodo {
            choice: agent::PlanTodoChoice::Accept { target, .. },
            ..
        } if !valid_target(target) => Err("invalid_request"),
        MobileCommand::GeneratePlan {
            priority: Some(priority),
            ..
        } if !valid_id(&priority.plan_id) || priority.todo_index >= 8 => {
            Err("invalid_plan_priority")
        }
        MobileCommand::GeneratePlan { direction, .. }
            if !agent::valid_plan_direction(direction) =>
        {
            Err("invalid_plan_input")
        }
        MobileCommand::DecideApproval { expected, .. } if !expected.valid() => {
            Err("invalid_request")
        }
        MobileCommand::CreateCard {
            title,
            definition_id,
            ..
        } if definition_id.is_some_and(|id| id <= 0)
            || definition_id.is_some() && title.is_none()
            || title.as_ref().is_some_and(|t| {
                t.trim().is_empty() || t.chars().count() > 80 || t.chars().any(char::is_control)
            }) =>
        {
            Err("invalid_request")
        }
        MobileCommand::ThreadFollowup { text, .. }
        | MobileCommand::SendMessage { text, .. }
        | MobileCommand::QueueMessage { text, .. }
            if text.trim().is_empty()
                || text.len() > 32 * 1024
                || text
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t') =>
        {
            Err("invalid_message")
        }
        MobileCommand::DeleteCard { expected, .. }
        | MobileCommand::ThreadFollowup { expected, .. }
        | MobileCommand::FeedbackDelivery { expected, .. }
        | MobileCommand::SendMessage { expected, .. }
        | MobileCommand::QueueMessage { expected, .. }
        | MobileCommand::InterruptTurn { expected }
            if !valid_target(expected) =>
        {
            Err("invalid_request")
        }
        MobileCommand::InterruptTurn { expected }
            if expected.run_id.is_none()
                || expected.session_id.is_none()
                || expected.turn_id.is_none() =>
        {
            Err("invalid_request")
        }
        MobileCommand::QueueMessage { expected, .. }
            if expected.run_id.is_none() || expected.session_id.is_none() =>
        {
            Err("invalid_request")
        }
        _ => Ok(()),
    }
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn valid_target(target: &norma::mobile::CardSnapshot) -> bool {
    !(target.node_id == 0
        || target.title.chars().count() > 256
        || target
            .task_title
            .as_ref()
            .is_some_and(|s| s.chars().count() > 1024)
        || target.run_id.as_ref().is_some_and(|s| !valid_id(s))
        || target.session_id.as_ref().is_some_and(|s| !valid_id(s))
        || target.turn_id.as_ref().is_some_and(|s| !valid_id(s)))
}
