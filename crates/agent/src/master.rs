use crate::rig_tools::{
    AssignAgentTaskTool, CancelAgentTaskTool, CreateAgentTool, CreateTerminalTool,
    DeleteCanvasCardTool, FindAgentTasksTool, FocusCardTool, RigToolContext, SendTerminalInputTool,
};
use crate::{
    norma_system_prompt, AgentCardAdapter, AgentRosterEntry, CurrentCanvasContext,
    NormaToolRuntime, ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::StreamExt;
use rig_core::agent::{
    Agent, AgentBuilder, AgentHook, Flow, HookContext, MultiTurnStreamItem, StepEvent,
};
use rig_core::client::CompletionClient;
use rig_core::completion::{CompletionModel, GetTokenUsage};
use rig_core::message::{Message, UserContent};
use rig_core::providers::{ollama, openai};
use rig_core::streaming::{StreamedAssistantContent, StreamingPrompt};
use rig_core::tool::ToolCallExtensions;
use rig_core::wasm_compat::{WasmCompatSend, WasmCompatSync};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

pub use crate::reply_visibility::visible_norma_reply;
use crate::reply_visibility::{finalize_stream_deltas, visible_stream_snapshot};

const MAX_MODEL_TURNS: usize = 12;
const MAX_TOOL_CALLS: usize = 20;
const TURN_TIMEOUT: Duration = Duration::from_secs(60);
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PROTOCOL_MESSAGES: usize = 96;
const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 16_384;
const HISTORY_CONTEXT_FRACTION_NUMERATOR: usize = 3;
const HISTORY_CONTEXT_FRACTION_DENOMINATOR: usize = 4;
const CONSERVATIVE_CHARS_PER_TOKEN: usize = 2;

#[derive(Clone)]
enum NormaModel {
    OpenAi(openai::completion::CompletionModel),
    Ollama(ollama::CompletionModel),
}

pub struct ModelFactory;

impl ModelFactory {
    fn create(config: &ProviderConfig) -> Result<NormaModel, NormaSessionError> {
        config
            .validate()
            .map_err(|error| NormaSessionError::Configuration(error.to_string()))?;
        let http_client = reqwest::Client::builder()
            .timeout(PROVIDER_TIMEOUT)
            .build()
            .map_err(|error| NormaSessionError::Configuration(error.to_string()))?;
        match config.protocol() {
            ProviderProtocol::OpenAiCompatible => {
                let client = openai::Client::builder()
                    .api_key(config.api_key().unwrap_or_default())
                    .base_url(config.endpoint())
                    .http_client(http_client)
                    .build()
                    .map_err(|error| NormaSessionError::Configuration(error.to_string()))?
                    .completions_api();
                Ok(NormaModel::OpenAi(client.completion_model(config.model())))
            }
            ProviderProtocol::Ollama => {
                let client = ollama::Client::builder()
                    .api_key(config.api_key().unwrap_or_default())
                    .base_url(config.endpoint())
                    .http_client(http_client)
                    .build()
                    .map_err(|error| NormaSessionError::Configuration(error.to_string()))?;
                Ok(NormaModel::Ollama(client.completion_model(config.model())))
            }
        }
    }
}

#[derive(Clone)]
pub struct NormaCancelHandle {
    active_turn: Arc<Mutex<CancellationToken>>,
}

impl NormaCancelHandle {
    pub fn cancel(&self) {
        self.active_turn
            .lock()
            .expect("Norma active turn lock should not be poisoned")
            .cancel();
    }
}

pub struct NormaSession {
    project_id: String,
    model: NormaModel,
    tools: NormaToolRuntime,
    canvas: CurrentCanvasContext,
    roster: Vec<AgentRosterEntry>,
    protocol_history: Vec<Message>,
    history_char_budget: usize,
    next_turn_id: u64,
    active_turn: Arc<Mutex<CancellationToken>>,
}

impl NormaSession {
    pub fn new(
        project_id: impl Into<String>,
        config: &ProviderConfig,
        adapter: Arc<dyn AgentCardAdapter>,
    ) -> Result<Self, NormaSessionError> {
        let project_id = project_id.into();
        let model = ModelFactory::create(config)?;
        Ok(Self {
            canvas: CurrentCanvasContext {
                project_name: project_id.clone(),
                workspace_root: project_id.clone(),
                total_cards: 0,
                cards: Vec::new(),
            },
            project_id,
            model,
            tools: NormaToolRuntime::new(adapter),
            roster: Vec::new(),
            protocol_history: Vec::new(),
            history_char_budget: history_char_budget(config),
            next_turn_id: 1,
            active_turn: Arc::new(Mutex::new(CancellationToken::new())),
        })
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn history_len(&self) -> usize {
        self.protocol_history.len()
    }

    pub fn cancel_handle(&self) -> NormaCancelHandle {
        NormaCancelHandle {
            active_turn: Arc::clone(&self.active_turn),
        }
    }

    pub fn update_environment(
        &mut self,
        canvas: CurrentCanvasContext,
        roster: Vec<AgentRosterEntry>,
    ) {
        self.canvas = canvas;
        self.roster = roster;
    }

    pub fn switch_model(&mut self, config: &ProviderConfig) -> Result<(), NormaSessionError> {
        let replacement = ModelFactory::create(config)?;
        self.cancel_handle().cancel();
        self.model = replacement;
        self.history_char_budget = history_char_budget(config);
        self.prune_protocol_history();
        Ok(())
    }

    pub async fn turn(&mut self, input: &str) -> Result<NormaTurnResult, NormaSessionError> {
        self.turn_with_deltas(input, None).await
    }

    pub async fn turn_streaming(
        &mut self,
        input: &str,
        deltas: tokio::sync::mpsc::Sender<String>,
    ) -> Result<NormaTurnResult, NormaSessionError> {
        self.turn_with_deltas(input, Some(deltas)).await
    }

    async fn turn_with_deltas(
        &mut self,
        input: &str,
        deltas: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> Result<NormaTurnResult, NormaSessionError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(NormaSessionError::EmptyInput);
        }
        let cancellation = self.begin_turn();
        let turn_id = format!("turn-{}", self.next_turn_id);
        self.next_turn_id = self.next_turn_id.saturating_add(1);
        let preamble = norma_system_prompt(&self.canvas, &self.roster);
        let contextual_delete_targets = self
            .canvas
            .cards
            .iter()
            .filter(|card| card.focused || card.selected)
            .map(|card| card.node_id)
            .collect();
        let context = ToolContext {
            project_id: self.project_id.clone(),
            turn_id,
            user_input: input.to_string(),
            contextual_delete_targets,
            tool_call_id: String::new(),
            operation_id: None,
            cancellation: cancellation.clone(),
        };
        let history = self.protocol_history.clone();
        let result = match &self.model {
            NormaModel::OpenAi(model) => {
                run_turn(
                    model.clone(),
                    self.tools.clone(),
                    preamble,
                    input,
                    history,
                    context,
                    cancellation,
                    deltas,
                )
                .await
            }
            NormaModel::Ollama(model) => {
                run_turn(
                    model.clone(),
                    self.tools.clone(),
                    preamble,
                    input,
                    history,
                    context,
                    cancellation,
                    deltas,
                )
                .await
            }
        }?;
        self.replace_protocol_history(result.new_messages);
        let final_text = visible_norma_reply(&result.final_text);
        if final_text.is_empty() {
            return Err(NormaSessionError::MissingFinalResponse);
        }
        Ok(NormaTurnResult {
            final_text,
            tool_calls: result.tool_calls,
        })
    }

    fn begin_turn(&self) -> CancellationToken {
        let next = CancellationToken::new();
        let mut active = self
            .active_turn
            .lock()
            .expect("Norma active turn lock should not be poisoned");
        active.cancel();
        *active = next.clone();
        next
    }

    fn replace_protocol_history(&mut self, complete_history: Vec<Message>) {
        self.protocol_history = complete_history;
        self.prune_protocol_history();
    }

    fn prune_protocol_history(&mut self) {
        while self.protocol_history.len() > MAX_PROTOCOL_MESSAGES
            || protocol_history_chars(&self.protocol_history) > self.history_char_budget
        {
            let next_turn = self
                .protocol_history
                .iter()
                .enumerate()
                .skip(1)
                .find_map(|(index, message)| is_human_user_message(message).then_some(index));
            match next_turn {
                Some(start) => {
                    self.protocol_history.drain(..start);
                }
                None => {
                    self.protocol_history.clear();
                    break;
                }
            }
        }
    }
}

fn history_char_budget(config: &ProviderConfig) -> usize {
    config
        .context_window_tokens()
        .unwrap_or(DEFAULT_CONTEXT_WINDOW_TOKENS)
        .saturating_mul(HISTORY_CONTEXT_FRACTION_NUMERATOR)
        / HISTORY_CONTEXT_FRACTION_DENOMINATOR
        * CONSERVATIVE_CHARS_PER_TOKEN
}

fn protocol_history_chars(history: &[Message]) -> usize {
    history
        .iter()
        .map(|message| serde_json::to_string(message).map_or(0, |serialized| serialized.len()))
        .sum()
}

fn is_human_user_message(message: &Message) -> bool {
    matches!(
        message,
        Message::User { content }
            if content
                .iter()
                .any(|item| !matches!(item, UserContent::ToolResult(_)))
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormaTurnResult {
    pub final_text: String,
    pub tool_calls: usize,
}

struct InternalTurnResult {
    final_text: String,
    tool_calls: usize,
    new_messages: Vec<Message>,
}

#[derive(Debug, Error)]
pub enum NormaSessionError {
    #[error("invalid Norma model configuration: {0}")]
    Configuration(String),
    #[error("Norma input must not be empty")]
    EmptyInput,
    #[error("Norma turn was cancelled")]
    Cancelled,
    #[error("Norma turn exceeded the 60 second budget")]
    Timeout,
    #[error("Norma exceeded the tool-call budget of {MAX_TOOL_CALLS}")]
    ToolBudgetExceeded,
    #[error("Norma stopped after 3 consecutive tool failures")]
    ToolFailureBudgetExceeded,
    #[error("Norma provider or tool loop failed: {0}")]
    Runtime(String),
    #[error("Norma returned no final response")]
    MissingFinalResponse,
    #[error("Norma tool side effect is unknown; the tool chain stopped without retrying")]
    UnknownSideEffect,
}

async fn run_turn<M>(
    model: M,
    tools: NormaToolRuntime,
    preamble: String,
    input: &str,
    history: Vec<Message>,
    context: ToolContext,
    cancellation: CancellationToken,
    deltas: Option<tokio::sync::mpsc::Sender<String>>,
) -> Result<InternalTurnResult, NormaSessionError>
where
    M: CompletionModel + Clone + 'static,
    M::StreamingResponse: WasmCompatSend + WasmCompatSync + GetTokenUsage + Clone + Unpin,
{
    let future = async move {
        let rig_context = RigToolContext::new(context);
        let unknown_status = rig_context.clone();
        let agent = build_agent(model, &preamble, tools, rig_context.clone());
        let mut extensions = ToolCallExtensions::new();
        extensions.insert(rig_context);
        let mut stream = agent
            .stream_prompt(input)
            .history(history)
            .tool_extensions(extensions)
            .max_turns(MAX_MODEL_TURNS)
            .tool_concurrency(1)
            .await;
        let mut tool_calls = 0usize;
        let mut pending_raw = String::new();
        let mut pending_visible = String::new();
        let mut pending_deltas = Vec::new();
        while let Some(item) = stream.next().await {
            if unknown_status.unknown_seen() {
                return Err(NormaSessionError::UnknownSideEffect);
            }
            if unknown_status.failure_budget_exhausted() {
                return Err(NormaSessionError::ToolFailureBudgetExceeded);
            }
            let item = item.map_err(|error| NormaSessionError::Runtime(error.to_string()))?;
            match item {
                MultiTurnStreamItem::ToolExecutionStart { .. } => {
                    pending_raw.clear();
                    pending_visible.clear();
                    pending_deltas.clear();
                    tool_calls = tool_calls.saturating_add(1);
                    if tool_calls > MAX_TOOL_CALLS {
                        return Err(NormaSessionError::ToolBudgetExceeded);
                    }
                }
                MultiTurnStreamItem::FinalResponse(response) => {
                    let final_visible = visible_norma_reply(&response.output);
                    if let Some(sender) = &deltas {
                        for delta in
                            finalize_stream_deltas(&final_visible, &pending_visible, pending_deltas)
                        {
                            let _ = sender.send(delta).await;
                        }
                    }
                    return Ok(InternalTurnResult {
                        final_text: response.output,
                        tool_calls,
                        new_messages: response.messages.unwrap_or_default(),
                    });
                }
                MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(text)) => {
                    pending_raw.push_str(&text.text);
                    let next_visible = visible_stream_snapshot(&pending_raw).trim().to_string();
                    if let Some(delta) = next_visible.strip_prefix(&pending_visible) {
                        if !delta.is_empty() {
                            pending_deltas.push(delta.to_string());
                            pending_visible = next_visible;
                        }
                    }
                }
                MultiTurnStreamItem::StreamAssistantItem(_)
                | MultiTurnStreamItem::StreamUserItem(_)
                | MultiTurnStreamItem::CompletionCall(_) => {}
                _ => {}
            }
        }
        if unknown_status.unknown_seen() {
            return Err(NormaSessionError::UnknownSideEffect);
        }
        if unknown_status.failure_budget_exhausted() {
            return Err(NormaSessionError::ToolFailureBudgetExceeded);
        }
        Err(NormaSessionError::MissingFinalResponse)
    };

    tokio::select! {
        _ = cancellation.cancelled() => Err(NormaSessionError::Cancelled),
        timed = tokio::time::timeout(TURN_TIMEOUT, future) => {
            timed.map_err(|_| NormaSessionError::Timeout)?
        }
    }
}

struct ToolCallIdentityHook {
    context: RigToolContext,
}

impl<M> AgentHook<M> for ToolCallIdentityHook
where
    M: CompletionModel,
{
    async fn on_event(&self, _context: &HookContext, event: StepEvent<'_, M>) -> Flow {
        if let StepEvent::ToolCall {
            tool_call_id,
            internal_call_id,
            ..
        } = event
        {
            self.context
                .record_tool_call(tool_call_id, internal_call_id);
        }
        Flow::cont()
    }
}

fn build_agent<M>(
    model: M,
    preamble: &str,
    tools: NormaToolRuntime,
    rig_context: RigToolContext,
) -> Agent<M>
where
    M: CompletionModel,
{
    AgentBuilder::new(model)
        .name("Norma")
        .description("Norma OS 的唯一 Master Agent")
        .preamble(preamble)
        .add_hook(ToolCallIdentityHook {
            context: rig_context,
        })
        .tool(FindAgentTasksTool::new(tools.clone()))
        .tool(CreateAgentTool::new(tools.clone()))
        .tool(AssignAgentTaskTool::new(tools.clone()))
        .tool(CancelAgentTaskTool::new(tools.clone()))
        .tool(CreateTerminalTool::new(tools.clone()))
        .tool(FocusCardTool::new(tools.clone()))
        .tool(SendTerminalInputTool::new(tools.clone()))
        .tool(DeleteCanvasCardTool::new(tools))
        .build()
}

#[cfg(test)]
#[path = "../tests/unit/master_tests.rs"]
mod tests;
