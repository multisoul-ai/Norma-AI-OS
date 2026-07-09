use llm::{
    card_tool_specs, parse_card_tool_call, tool_result_json_for_command_result, ChatMessage,
    ChatRequest, LlmClient, LlmError, Role,
};
use norma::protocol::{CardCommand, CardCommandResult};

use crate::DialogueError;

#[derive(Debug)]
pub struct TurnOutcome {
    pub reply_text: String,
    pub executed_commands: Vec<CardCommand>,
    pub command_results: Vec<CardCommandResult>,
}

pub trait ToolExecutor {
    fn execute(&mut self, command: CardCommand) -> Result<CardCommandResult, DialogueError>;
}

pub fn run_voice_turn<L: LlmClient, E: ToolExecutor>(
    llm: &mut L,
    executor: &mut E,
    user_transcript: &str,
    system_prompt: &str,
    max_tool_rounds: usize,
) -> Result<TurnOutcome, DialogueError> {
    let mut messages = vec![
        ChatMessage {
            role: Role::System,
            content: system_prompt.to_string(),
            tool_call_id: None,
            tool_calls: vec![],
        },
        ChatMessage {
            role: Role::User,
            content: user_transcript.to_string(),
            tool_call_id: None,
            tool_calls: vec![],
        },
    ];
    let mut executed_commands = Vec::new();
    let mut command_results = Vec::new();

    for _ in 0..max_tool_rounds {
        let response = llm
            .chat(&ChatRequest {
                messages: messages.clone(),
                tools: card_tool_specs(),
            })
            .map_err(map_llm_err)?;

        if response.tool_calls.is_empty() {
            return Ok(TurnOutcome {
                reply_text: response.assistant_text,
                executed_commands,
                command_results,
            });
        }

        messages.push(ChatMessage {
            role: Role::Assistant,
            content: response.assistant_text,
            tool_call_id: None,
            tool_calls: response.tool_calls.clone(),
        });

        for tool_call in response.tool_calls {
            let command = parse_card_tool_call(&tool_call).map_err(map_llm_err)?;
            let result = executor.execute(command.clone())?;
            executed_commands.push(command);
            command_results.push(result.clone());

            messages.push(ChatMessage {
                role: Role::Tool,
                content: tool_result_json_for_command_result(&result),
                tool_call_id: Some(tool_call.id),
                tool_calls: vec![],
            });
        }
    }

    Err(DialogueError::Message(format!(
        "max tool rounds exceeded ({max_tool_rounds})"
    )))
}

fn map_llm_err(err: LlmError) -> DialogueError {
    DialogueError::Message(err.to_string())
}

#[cfg(test)]
#[path = "../tests/unit/turn_tests.rs"]
mod turn_tests;
