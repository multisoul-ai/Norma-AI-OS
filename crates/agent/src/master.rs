mod context_session;
mod request_budget;
mod turn;
use turn::run_turn;
mod error;
pub use error::NormaSessionError;
mod delegation;
mod memory_session;
use crate::master_history::{history_char_budget, is_human_user_message, protocol_history_chars};
use crate::rig_tools::{
    AssignAgentTaskTool, CancelAgentReplyTool, CancelAgentTaskTool, CreateAgentTool,
    CreateBrowserTool, CreatePreviewTool, CreateTerminalTool, DeleteCanvasCardTool,
    FindAgentTasksTool, FocusCardTool, ReplyAgentTool, RigToolContext, SendTerminalInputTool,
};
use crate::{
    AgentCardAdapter, AgentRosterEntry, CurrentCanvasContext, DesktopContext, NormaToolRuntime,
    ProviderConfig, ProviderProtocol, ToolContext,
};
pub use delegation::NormaDelegation;
use futures::StreamExt;
use rig_core::agent::{
    Agent, AgentBuilder, AgentHook, Flow, HookContext, MultiTurnStreamItem, StepEvent,
};
use rig_core::client::CompletionClient;
use rig_core::completion::{CompletionModel, GetTokenUsage};
use rig_core::message::Message;
use rig_core::providers::{ollama, openai};
use rig_core::streaming::{StreamedAssistantContent, StreamingPrompt};
use rig_core::tool::ToolCallExtensions;
use rig_core::wasm_compat::{WasmCompatSend, WasmCompatSync};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub use crate::reply_visibility::visible_norma_reply;
use crate::reply_visibility::{finalize_stream_deltas, visible_stream_snapshot};

const MAX_MODEL_TURNS: usize = 12;
const MAX_TOOL_CALLS: usize = crate::MAX_TOOL_CALLS_PER_TURN;
const TURN_TIMEOUT: Duration = Duration::from_secs(60);
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PROTOCOL_MESSAGES: usize = 96;

#[derive(Clone)]
pub(crate) enum NormaModel {
    OpenAi(openai::completion::CompletionModel),
    Ollama(ollama::CompletionModel),
}

pub struct ModelFactory;

impl ModelFactory {
    pub(crate) fn create(config: &ProviderConfig) -> Result<NormaModel, NormaSessionError> {
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
    model_name: String,
    tools: NormaToolRuntime,
    canvas: CurrentCanvasContext,
    desktop_context: Option<DesktopContext>,
    roster: Vec<AgentRosterEntry>,
    protocol_history: Vec<Message>,
    session_id: String,
    memory_enabled: bool,
    memory_epoch: u64,
    memory_revisions: Vec<(crate::MemoryScope, crate::MemoryRevision)>,
    context_window_tokens: usize,
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
                conversation_tails: Vec::new(),
            },
            desktop_context: None,
            project_id,
            model,
            model_name: config.model().to_owned(),
            tools: NormaToolRuntime::new(adapter),
            roster: Vec::new(),
            protocol_history: Vec::new(),
            session_id: uuid::Uuid::new_v4().to_string(),
            memory_enabled: false,
            memory_epoch: 0,
            memory_revisions: Vec::new(),
            context_window_tokens: config.context_window_tokens().unwrap_or(16_384),
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

    pub fn update_desktop_context(&mut self, context: Option<DesktopContext>) {
        self.desktop_context = context;
    }

    pub fn switch_model(&mut self, config: &ProviderConfig) -> Result<(), NormaSessionError> {
        let replacement = ModelFactory::create(config)?;
        self.cancel_handle().cancel();
        self.model = replacement;
        self.model_name = config.model().to_owned();
        self.context_window_tokens = config.context_window_tokens().unwrap_or(16_384);
        self.history_char_budget = history_char_budget(config);
        self.prune_protocol_history();
        Ok(())
    }

    pub async fn turn(&mut self, input: &str) -> Result<NormaTurnResult, NormaSessionError> {
        self.turn_with_deltas(input, None, crate::AgentInputSource::Other, None)
            .await
    }

    pub async fn turn_streaming(
        &mut self,
        input: &str,
        deltas: tokio::sync::mpsc::Sender<String>,
        input_source: crate::AgentInputSource,
    ) -> Result<NormaTurnResult, NormaSessionError> {
        self.turn_with_deltas(input, Some(deltas), input_source, None)
            .await
    }

    async fn turn_with_deltas(
        &mut self,
        input: &str,
        deltas: Option<tokio::sync::mpsc::Sender<String>>,
        input_source: crate::AgentInputSource,
        delegation: Option<NormaDelegation>,
    ) -> Result<NormaTurnResult, NormaSessionError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(NormaSessionError::EmptyInput);
        }
        let cancellation = self.begin_turn(
            delegation
                .as_ref()
                .map(|request| request.cancellation.child_token()),
        );
        delegation::with_turn_budget(cancellation.clone(), async {
            let turn_id = delegation.as_ref().map_or_else(
                || format!("{}:turn-{}", self.session_id, self.next_turn_id),
                NormaDelegation::turn_id,
            );
            self.next_turn_id = self.next_turn_id.saturating_add(1);
            let model_input = delegation
                .as_ref()
                .map_or_else(|| input.to_string(), NormaDelegation::model_input);
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
                input_source,
                contextual_delete_targets,
                tool_call_id: String::new(),
                operation_id: None,
                cancellation: cancellation.clone(),
            };
            let frame = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(NormaSessionError::Cancelled),
                frame = self.tools.shared_context(&context) => frame.map_err(|e| NormaSessionError::Runtime(e.to_string()))?,
            };
            let (preamble, context_data) = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(NormaSessionError::Cancelled),
                preamble = self.memory_preamble(&model_input, &context, frame) => preamble?,
            };
            if let Some(final_text) = crate::direct_address_turn::maybe_handle_direct_address(
                &self.tools,
                &context,
                input,
                &self.roster,
            )
            .await?
            {
                let final_text = visible_norma_reply(&final_text);
                if final_text.is_empty() {
                    return Err(NormaSessionError::MissingFinalResponse);
                }
                return Ok(NormaTurnResult {
                    final_text,
                    tool_calls: 0,
                });
            }
            let history = self.protocol_history.clone();
            let request = TurnRequest {
                preamble,
                context_data,
                model_name: self.model_name.clone(),
                max_input_bytes: request_budget::input_bytes(self.context_window_tokens),
                input: model_input,
                history,
                context,
                cancellation,
                deltas,
                desktop_active: matches!(
                    self.desktop_context,
                    Some(
                        DesktopContext::WorkspaceOverview { .. }
                            | DesktopContext::ProjectCanvas { .. }
                            | DesktopContext::Switching { .. }
                    )
                ),
            };
            let result = match &self.model {
                NormaModel::OpenAi(model) => {
                    run_turn(model.clone(), self.tools.clone(), request).await
                }
                NormaModel::Ollama(model) => {
                    run_turn(model.clone(), self.tools.clone(), request).await
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
        })
        .await
    }

    fn begin_turn(&self, requested: Option<CancellationToken>) -> CancellationToken {
        let next = requested.unwrap_or_default();
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

struct TurnRequest {
    preamble: String,
    context_data: Option<String>,
    model_name: String,
    max_input_bytes: usize,
    input: String,
    history: Vec<Message>,
    context: ToolContext,
    cancellation: CancellationToken,
    deltas: Option<tokio::sync::mpsc::Sender<String>>,
    desktop_active: bool,
}

use memory_session::build_agent;

#[cfg(test)]
#[path = "../tests/unit/master_tests.rs"]
mod tests;
