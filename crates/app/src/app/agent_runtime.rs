use crate::app::MultisoulApp;
use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCommand, AgentEvent, AgentInputSource,
};
use futures::future::BoxFuture;
use gpui::{Context, Window};
use norma::protocol::{CardCommand, CardCommandResult, CardKind, Placement};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

mod adapter_task;
mod artifacts;
mod command_protocol;
mod command_support;
mod commands;
mod context_commands;
mod context_guard;
pub(crate) use context_guard::execute_norma_adapter_command_with_source;
pub(crate) use context_guard::prepare_services as prepare_context_services;
mod conversation_tails;
mod coordination;
mod create_agent_command;
mod delete_card;
mod diagnostics;
mod dispatch_effects;
mod doubao_adapter;
mod gpt_live_adapter;
mod idea_commands;
mod identity;
mod mobile_control;
mod mobile_delivery;
mod mobile_message;
mod orchestration_commands;
pub(super) mod pending;
mod reply_adapter;
mod session_reply_queue;
mod session_titles;

pub(crate) use conversation_tails::{ConversationTailMemory, ConversationTailWriter};
pub(crate) use identity::{self_dispatch_forbidden_error, AgentIdentitySnapshot};
pub(crate) use session_reply_queue::AgentSessionReplyQueue;

pub(crate) use artifacts::artifacts_from_agent_events;
#[cfg(any(test, feature = "e2e-test-support"))]
pub(crate) use commands::execute_agent_card_command;
use context_commands::contextualized_agent_commands;
#[cfg(test)]
use coordination::agent_completion_report;
pub(crate) use doubao_adapter::start_doubao_adapter;
pub(crate) use gpt_live_adapter::{start_gpt_live_adapter, LiveAdapterScope};
pub(crate) use idea_commands::assign_idea_for_ui;
pub(crate) use orchestration_commands::execute_high_level_agent_command;
pub(crate) use orchestration_commands::execute_orchestration_agent_command;
pub(crate) use orchestration_commands::execute_orchestration_agent_command_with_visual_origin;

static NEXT_RUNTIME_ID: AtomicU64 = AtomicU64::new(1);
mod runtime_service;
pub(crate) use runtime_service::shared_tokio_runtime;

struct AgentRequest {
    command: AgentCommand,
    response: async_channel::Sender<Result<Vec<AgentEvent>, String>>,
}

struct NormaAdapterRequest {
    context: agent::ToolContext,
    command: AdapterCommand,
    result_tx: tokio::sync::oneshot::Sender<Result<AdapterResult, AdapterError>>,
}

#[derive(Clone)]
struct TurnRuntime {
    session: Arc<tokio::sync::Mutex<agent::NormaSession>>,
    events: async_channel::Sender<AgentEvent>,
    dialogue_cancelled: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
    cancelled: CancellationToken,
}

#[derive(Clone)]
struct ChannelAgentCardAdapter {
    sender: async_channel::Sender<NormaAdapterRequest>,
}

impl agent::AgentCardAdapter for ChannelAgentCardAdapter {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a agent::ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            let (result_tx, result_rx) = tokio::sync::oneshot::channel();
            let request = NormaAdapterRequest {
                context: context.clone(),
                command,
                result_tx,
            };
            tokio::select! {
                biased;
                _ = context.cancellation.cancelled() => return Err(AdapterError::Cancelled),
                sent = self.sender.send(request) => {
                    sent.map_err(|_| AdapterError::Failed("AgentCard Adapter 已关闭".to_string()))?;
                }
            }
            match result_rx.await {
                Ok(receipt) => receipt,
                Err(_) => Ok(AdapterResult::unknown(
                    "AgentCard Adapter 回执丢失，无法确认副作用是否发生",
                )),
            }
        })
    }
}

pub(crate) struct ProjectAgentRuntime {
    id: u64,
    project_id: String,
    context_scope: Option<agent::context::ContextScope>,
    config: Arc<Mutex<agent::ProviderConfig>>,
    request_tx: async_channel::Sender<AgentRequest>,
    cancelled: CancellationToken,
    dialogue_cancelled: Arc<AtomicBool>,
    workspace_memory_roots: Arc<Mutex<Vec<std::path::PathBuf>>>,
}

impl ProjectAgentRuntime {
    fn spawn(
        project_id: String,
        config: agent::ProviderConfig,
        paths: storage::DatabasePaths,
        root: Option<std::path::PathBuf>,
        shared: super::shared_context::SharedContext,
    ) -> Result<
        (
            Self,
            async_channel::Receiver<NormaAdapterRequest>,
            async_channel::Receiver<AgentEvent>,
        ),
        String,
    > {
        Self::spawn_scoped(project_id, config, paths, root, false, Some(shared))
    }

    fn spawn_scoped(
        project_id: String,
        config: agent::ProviderConfig,
        paths: storage::DatabasePaths,
        root: Option<std::path::PathBuf>,
        workspace: bool,
        shared: Option<super::shared_context::SharedContext>,
    ) -> Result<
        (
            Self,
            async_channel::Receiver<NormaAdapterRequest>,
            async_channel::Receiver<AgentEvent>,
        ),
        String,
    > {
        let runtime = shared_tokio_runtime()?;
        let context_scope = shared.as_ref().map(|service| service.scope().clone());
        let id = NEXT_RUNTIME_ID.fetch_add(1, Ordering::Relaxed);
        diagnostics::runtime_spawn(id, &config, true);
        let (request_tx, request_rx) = async_channel::bounded(16);
        let (adapter_tx, adapter_rx) = async_channel::bounded(1);
        let (event_tx, event_rx) = async_channel::bounded(32);
        let cancelled = CancellationToken::new();
        let dialogue_cancelled = Arc::new(AtomicBool::new(false));
        let active_config = Arc::new(Mutex::new(config.clone()));
        let workspace_memory_roots = Arc::new(Mutex::new(Vec::new()));
        let delegate: Arc<dyn agent::AgentCardAdapter> =
            Arc::new(ChannelAgentCardAdapter { sender: adapter_tx });
        let adapter = if workspace {
            super::memory::workspace_adapter(delegate, paths, workspace_memory_roots.clone())
        } else {
            super::memory::adapter(delegate, paths, root)
        };
        let adapter = if let Some(shared) = shared {
            super::shared_context::adapter::wrap(adapter, shared)
        } else {
            adapter
        };
        let session = agent::NormaSession::new(project_id.clone(), &config, adapter)
            .map_err(|error| error.to_string())?;
        runtime.spawn(run_project_agent(
            session,
            request_rx,
            event_tx,
            cancelled.clone(),
            Arc::clone(&dialogue_cancelled),
            Arc::clone(&active_config),
        ));
        Ok((
            Self {
                id,
                project_id,
                context_scope,
                config: active_config,
                request_tx,
                cancelled,
                dialogue_cancelled,
                workspace_memory_roots,
            },
            adapter_rx,
            event_rx,
        ))
    }

    fn matches_project(&self, project_id: &str) -> bool {
        self.project_id == project_id
    }

    fn uses_config(&self, config: &agent::ProviderConfig) -> bool {
        self.config
            .lock()
            .expect("Norma provider config lock should not be poisoned")
            .eq(config)
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn dialogue_cancel_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.dialogue_cancelled)
    }

    pub(crate) fn submit(
        &self,
        command: AgentCommand,
    ) -> async_channel::Receiver<Result<Vec<AgentEvent>, String>> {
        diagnostics::command_submit(self.id, &command);
        let (response_tx, response_rx) = async_channel::bounded(1);
        if self
            .request_tx
            .try_send(AgentRequest {
                command,
                response: response_tx.clone(),
            })
            .is_err()
        {
            let _ = response_tx.try_send(Err("Norma 当前不可用或请求队列已满".to_string()));
        }
        response_rx
    }
}

impl Drop for ProjectAgentRuntime {
    fn drop(&mut self) {
        self.cancelled.cancel();
        let (response, _) = async_channel::bounded(1);
        let _ = self.request_tx.try_send(AgentRequest {
            command: AgentCommand::ProjectClosed,
            response,
        });
    }
}

async fn run_project_agent(
    session: agent::NormaSession,
    requests: async_channel::Receiver<AgentRequest>,
    events: async_channel::Sender<AgentEvent>,
    cancelled: CancellationToken,
    dialogue_cancelled: Arc<AtomicBool>,
    active_config: Arc<Mutex<agent::ProviderConfig>>,
) {
    let session = Arc::new(tokio::sync::Mutex::new(session));
    let cancel_handle = session.lock().await.cancel_handle();
    let mut roster = Vec::new();
    let mut canvas = None;
    let mut desktop_context = None;
    let mut model_switch_error = None;
    let generation = Arc::new(AtomicU64::new(0));
    let turn_runtime = TurnRuntime {
        session: Arc::clone(&session),
        events: events.clone(),
        dialogue_cancelled: Arc::clone(&dialogue_cancelled),
        generation: Arc::clone(&generation),
        cancelled: cancelled.clone(),
    };
    let worker = async {
        while let Ok(request) = requests.recv().await {
            if cancelled.is_cancelled() {
                cancel_handle.cancel();
                let _ = request
                    .response
                    .send(Err("Norma Session 已关闭".to_string()))
                    .await;
                break;
            }
            match request.command {
                AgentCommand::UpdateAgentRoster(next) => {
                    roster = next;
                    let _ = request.response.send(Ok(Vec::new())).await;
                }
                AgentCommand::UpdateCurrentCanvasContext(next) => {
                    canvas = Some(next);
                    let _ = request.response.send(Ok(Vec::new())).await;
                }
                AgentCommand::UpdateDesktopContext(next) => {
                    desktop_context = next;
                    let _ = request.response.send(Ok(Vec::new())).await;
                }
                AgentCommand::SwitchModel(config) => {
                    cancel_handle.cancel();
                    generation.fetch_add(1, Ordering::AcqRel);
                    let result = session.lock().await.switch_model(&config);
                    match result {
                        Ok(()) => {
                            model_switch_error = None;
                            *active_config
                                .lock()
                                .expect("Norma provider config lock should not be poisoned") =
                                config;
                            let _ = request.response.send(Ok(Vec::new())).await;
                        }
                        Err(error) => {
                            let message = error.to_string();
                            model_switch_error = Some(message.clone());
                            let _ = request.response.send(Err(message)).await;
                        }
                    }
                }
                AgentCommand::UserTurn {
                    input,
                    input_source,
                } => {
                    if let Some(message) = model_switch_error.take() {
                        let _ = events
                            .send(AgentEvent::ProviderError {
                                message: message.clone(),
                            })
                            .await;
                        let _ = request.response.send(Err(message)).await;
                        continue;
                    }
                    cancel_handle.cancel();
                    dialogue_cancelled.store(false, Ordering::Release);
                    let turn_generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
                    let context = canvas.clone();
                    let desktop_context = desktop_context.clone();
                    let roster = roster.clone();
                    spawn_turn(
                        &turn_runtime,
                        input,
                        input_source,
                        context,
                        desktop_context,
                        roster,
                        request.response,
                        turn_generation,
                    );
                }
                AgentCommand::CancelLatest => {
                    cancel_handle.cancel();
                    let _ = request.response.send(Ok(Vec::new())).await;
                }
                AgentCommand::CancelTask { task_id } => {
                    cancel_handle.cancel();
                    dialogue_cancelled.store(false, Ordering::Release);
                    spawn_turn(
                        &turn_runtime,
                        format!("取消 Agent 任务 #{task_id}"),
                        AgentInputSource::Other,
                        canvas.clone(),
                        desktop_context.clone(),
                        roster.clone(),
                        request.response,
                        generation.fetch_add(1, Ordering::AcqRel) + 1,
                    );
                }
                AgentCommand::Status { task_id } => {
                    cancel_handle.cancel();
                    dialogue_cancelled.store(false, Ordering::Release);
                    let prompt = task_id.map_or_else(
                        || "查询最近未完成的 Agent 任务".to_string(),
                        |id| format!("查询 Agent 任务 #{id}"),
                    );
                    spawn_turn(
                        &turn_runtime,
                        prompt,
                        AgentInputSource::Other,
                        canvas.clone(),
                        desktop_context.clone(),
                        roster.clone(),
                        request.response,
                        generation.fetch_add(1, Ordering::AcqRel) + 1,
                    );
                }
                AgentCommand::CardActivity { node_id, summary } => {
                    let event = AgentEvent::CardActivityNotification { node_id, summary };
                    let _ = events.send(event.clone()).await;
                    let _ = request.response.send(Ok(vec![event])).await;
                }
                AgentCommand::ProjectClosed => {
                    cancel_handle.cancel();
                    let _ = request.response.send(Ok(Vec::new())).await;
                    break;
                }
            }
        }
    };
    tokio::select! {
        biased;
        _ = cancelled.cancelled() => cancel_handle.cancel(),
        _ = worker => {}
    }
}

mod turn_worker;
use turn_worker::spawn_turn;

mod submit;
mod workspace;
pub(crate) use workspace::{revoke_workspace_memory, submit_workspace_turn};

include!("agent_runtime/adapter_pump.rs");
include!("agent_runtime/adapter_execute.rs");
include!("agent_runtime/desktop_content_tools.rs");
include!("agent_runtime/pump_events.rs");

#[cfg(test)]
#[path = "agent_runtime/tests.rs"]
mod tests;
