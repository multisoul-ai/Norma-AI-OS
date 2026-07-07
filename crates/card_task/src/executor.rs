use crate::CardTaskStep;
use norma::protocol::{CardCommand, CardCommandError, CardCommandResult, NodeId};
use std::collections::HashMap;
use voice::{CompiledVoicePlan, ExecutableCommand};

pub trait CardCommandRunner {
    fn run(&mut self, command: CardCommand) -> Result<CardCommandResult, CardCommandError>;
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct CardTaskExecutionContext {
    pub created_by_step: HashMap<usize, NodeId>,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum CardTaskExecutionError {
    #[error("missing previous step {step}")]
    MissingPreviousStep { step: usize },
    #[error("previous step {step} did not create a node")]
    PreviousStepDidNotCreateNode { step: usize },
    #[error(transparent)]
    Command(#[from] CardCommandError),
}

pub fn compile_card_task_steps(compiled: &CompiledVoicePlan) -> Vec<CardTaskStep> {
    compiled
        .commands
        .iter()
        .cloned()
        .map(|command| match command {
            ExecutableCommand::Card(command) => CardTaskStep::Command(command),
            ExecutableCommand::SendTextToPrevious {
                previous_step,
                text,
                submit,
            } => CardTaskStep::SendTextToPrevious {
                previous_step,
                text,
                submit,
            },
            ExecutableCommand::FocusPrevious { previous_step } => {
                CardTaskStep::FocusPrevious { previous_step }
            }
        })
        .collect()
}

pub fn execute_card_task_step(
    step_index: usize,
    step: &CardTaskStep,
    total_steps: usize,
    context: &mut CardTaskExecutionContext,
    runner: &mut dyn CardCommandRunner,
) -> Result<CardCommandResult, CardTaskExecutionError> {
    let command = match step {
        CardTaskStep::Command(command) => command.clone(),
        CardTaskStep::SendTextToPrevious {
            previous_step,
            text,
            submit,
        } => CardCommand::SendText {
            node_id: resolve_previous_node(step_index, *previous_step, total_steps, context)?,
            text: text.clone(),
            submit: *submit,
        },
        CardTaskStep::FocusPrevious { previous_step } => CardCommand::Focus {
            node_id: resolve_previous_node(step_index, *previous_step, total_steps, context)?,
        },
    };

    let result = runner.run(command)?;
    if let CardCommandResult::Created { node_id } = result {
        context.created_by_step.insert(step_index, node_id);
        return Ok(CardCommandResult::Created { node_id });
    }
    Ok(result)
}

fn resolve_previous_node(
    step_index: usize,
    previous_step: usize,
    total_steps: usize,
    context: &CardTaskExecutionContext,
) -> Result<NodeId, CardTaskExecutionError> {
    if previous_step >= total_steps || previous_step >= step_index {
        return Err(CardTaskExecutionError::MissingPreviousStep {
            step: previous_step,
        });
    }

    context.created_by_step.get(&previous_step).copied().ok_or(
        CardTaskExecutionError::PreviousStepDidNotCreateNode {
            step: previous_step,
        },
    )
}
