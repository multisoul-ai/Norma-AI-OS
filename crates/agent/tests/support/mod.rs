use agent::{
    AgentEvent, AgentProvider, AgentSystem, CardTaskBudget, CardToolHost, CardToolRequest,
    CardToolResult, MemoryJournal, ProviderError, ProviderRequest, ProviderResponse,
    ProviderToolCall,
};
use norma::protocol::{
    AgentTaskReceipt, AgentTaskReceiptStatus, CardCommand, CardCommandResult, CardSummary,
};
use std::collections::VecDeque;
use std::sync::Once;

static TRACE_INIT: Once = Once::new();

pub fn init_trace() {
    TRACE_INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_test_writer()
            .without_time()
            .with_max_level(tracing::Level::TRACE)
            .try_init()
            .expect("test tracing subscriber should initialize");
    });
}

pub struct ScriptedProvider {
    responses: VecDeque<ProviderResponse>,
}

impl ScriptedProvider {
    pub fn new(responses: impl IntoIterator<Item = ProviderResponse>) -> Self {
        Self {
            responses: responses.into_iter().collect(),
        }
    }
}

impl Default for ScriptedProvider {
    fn default() -> Self {
        Self::new(Vec::<ProviderResponse>::new())
    }
}

impl AgentProvider for ScriptedProvider {
    fn complete(&mut self, _request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.responses
            .pop_front()
            .ok_or_else(|| ProviderError::Message("mock provider exhausted".to_string()))
    }
}

#[derive(Default)]
pub struct RecordingHost {
    pub commands: Vec<CardToolRequest>,
    pub next_node_id: u64,
    pub listed_cards: Vec<CardSummary>,
}

impl CardToolHost for RecordingHost {
    fn execute(
        &mut self,
        request: CardToolRequest,
    ) -> Result<CardToolResult, agent::CardToolError> {
        self.commands.push(request.clone());
        let result = match request.command {
            CardCommand::Create { .. } => {
                self.next_node_id += 1;
                CardCommandResult::Created {
                    node_id: self.next_node_id,
                }
            }
            CardCommand::SendText { node_id, .. } => CardCommandResult::Sent { node_id },
            CardCommand::Focus { node_id } => CardCommandResult::Focused { node_id },
            CardCommand::Delete { node_id } => CardCommandResult::Deleted { node_id },
            CardCommand::List => CardCommandResult::Listed {
                cards: self.listed_cards.clone(),
            },
            CardCommand::ListAgents => CardCommandResult::AgentsListed { agents: Vec::new() },
            CardCommand::CreateAgent { name, activate, .. } => {
                self.next_node_id += 1;
                CardCommandResult::AgentCreated {
                    definition_id: self.next_node_id as i64,
                    name: name.unwrap_or_else(|| "Alan".to_string()),
                    node_id: activate.then_some(self.next_node_id),
                }
            }
            CardCommand::ActivateAgent { name } => {
                self.next_node_id += 1;
                CardCommandResult::AgentActivated {
                    definition_id: self.next_node_id as i64,
                    name,
                    node_id: self.next_node_id,
                    reused: false,
                }
            }
            CardCommand::DispatchAgentTasks { assignments, .. } => {
                CardCommandResult::AgentTasksDispatched {
                    receipts: assignments
                        .into_iter()
                        .map(|assignment| AgentTaskReceipt {
                            name: assignment.name,
                            node_id: 1,
                            task: assignment.task,
                            status: AgentTaskReceiptStatus::Submitted,
                        })
                        .collect(),
                }
            }
        };
        Ok(CardToolResult { result })
    }
}

pub fn tool(id: &str, name: &str, arguments: &str) -> ProviderToolCall {
    ProviderToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments_json: arguments.to_string(),
    }
}

pub fn response(tool_calls: impl IntoIterator<Item = ProviderToolCall>) -> ProviderResponse {
    ProviderResponse {
        text: String::new(),
        tool_calls: tool_calls.into_iter().collect(),
    }
}

pub fn text_response(text: &str) -> ProviderResponse {
    ProviderResponse {
        text: text.to_string(),
        tool_calls: Vec::new(),
    }
}

pub fn system(
    dialogue: ScriptedProvider,
    card: ScriptedProvider,
    host: RecordingHost,
) -> AgentSystem<ScriptedProvider, ScriptedProvider, RecordingHost, MemoryJournal> {
    AgentSystem::new(
        "project-a",
        dialogue,
        card,
        host,
        MemoryJournal::default(),
        CardTaskBudget::default(),
    )
}

pub fn task_id_from(events: &[AgentEvent]) -> u64 {
    events
        .iter()
        .find_map(|event| match event {
            AgentEvent::TaskQueued { task_id, .. } => Some(*task_id),
            _ => None,
        })
        .expect("任务入队事件必须包含 task id")
}
