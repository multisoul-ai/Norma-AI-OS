use crate::app::MultisoulApp;
use agent::{AdapterCommand, AdapterError, AdapterResult, AgentCommand, AgentEvent};
use futures::future::BoxFuture;
use gpui::{Context, Window};
use norma::protocol::{AgentTaskAssignment, CardCommand, CardCommandResult, CardKind, Placement};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

mod artifacts;
mod commands;
mod context_commands;
mod conversation_tails;
mod coordination;
mod diagnostics;
pub(super) mod pending;
mod reply_adapter;
mod session_reply_queue;

pub(crate) use conversation_tails::{ConversationTailMemory, ConversationTailWriter};
pub(crate) use session_reply_queue::AgentSessionReplyQueue;

pub(crate) use artifacts::artifacts_from_agent_events;
#[cfg(any(test, feature = "e2e-test-support"))]
pub(crate) use commands::execute_agent_card_command;
use context_commands::contextualized_agent_commands;
#[cfg(test)]
use coordination::agent_completion_report;

static NEXT_RUNTIME_ID: AtomicU64 = AtomicU64::new(1);
static TOKIO_RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();

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
    config: Arc<Mutex<agent::ProviderConfig>>,
    request_tx: async_channel::Sender<AgentRequest>,
    cancelled: Arc<AtomicBool>,
    dialogue_cancelled: Arc<AtomicBool>,
}

impl ProjectAgentRuntime {
    fn spawn(
        project_id: String,
        config: agent::ProviderConfig,
    ) -> Result<
        (
            Self,
            async_channel::Receiver<NormaAdapterRequest>,
            async_channel::Receiver<AgentEvent>,
        ),
        String,
    > {
        let runtime = shared_tokio_runtime()?;
        let id = NEXT_RUNTIME_ID.fetch_add(1, Ordering::Relaxed);
        diagnostics::runtime_spawn(id, &config, true);
        let (request_tx, request_rx) = async_channel::bounded(16);
        let (adapter_tx, adapter_rx) = async_channel::bounded(1);
        let (event_tx, event_rx) = async_channel::bounded(32);
        let cancelled = Arc::new(AtomicBool::new(false));
        let dialogue_cancelled = Arc::new(AtomicBool::new(false));
        let active_config = Arc::new(Mutex::new(config.clone()));
        let session = agent::NormaSession::new(
            project_id.clone(),
            &config,
            Arc::new(ChannelAgentCardAdapter { sender: adapter_tx }),
        )
        .map_err(|error| error.to_string())?;
        runtime.spawn(run_project_agent(
            session,
            request_rx,
            event_tx,
            Arc::clone(&cancelled),
            Arc::clone(&dialogue_cancelled),
            Arc::clone(&active_config),
        ));
        Ok((
            Self {
                id,
                project_id,
                config: active_config,
                request_tx,
                cancelled,
                dialogue_cancelled,
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
        self.cancelled.store(true, Ordering::Release);
        let (response, _) = async_channel::bounded(1);
        let _ = self.request_tx.try_send(AgentRequest {
            command: AgentCommand::ProjectClosed,
            response,
        });
    }
}

fn shared_tokio_runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    TOKIO_RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .thread_name("norma-agent-runtime")
                .enable_all()
                .build()
                .map_err(|error| format!("无法启动 Norma async runtime：{error}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

async fn run_project_agent(
    session: agent::NormaSession,
    requests: async_channel::Receiver<AgentRequest>,
    events: async_channel::Sender<AgentEvent>,
    cancelled: Arc<AtomicBool>,
    dialogue_cancelled: Arc<AtomicBool>,
    active_config: Arc<Mutex<agent::ProviderConfig>>,
) {
    let session = Arc::new(tokio::sync::Mutex::new(session));
    let cancel_handle = session.lock().await.cancel_handle();
    let mut roster = Vec::new();
    let mut canvas = None;
    let mut model_switch_error = None;
    let generation = Arc::new(AtomicU64::new(0));
    let turn_runtime = TurnRuntime {
        session: Arc::clone(&session),
        events: events.clone(),
        dialogue_cancelled: Arc::clone(&dialogue_cancelled),
        generation: Arc::clone(&generation),
    };
    while let Ok(request) = requests.recv().await {
        if cancelled.load(Ordering::Acquire) {
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
            AgentCommand::SwitchModel(config) => {
                cancel_handle.cancel();
                generation.fetch_add(1, Ordering::AcqRel);
                let result = session.lock().await.switch_model(&config);
                match result {
                    Ok(()) => {
                        model_switch_error = None;
                        *active_config
                            .lock()
                            .expect("Norma provider config lock should not be poisoned") = config;
                        let _ = request.response.send(Ok(Vec::new())).await;
                    }
                    Err(error) => {
                        let message = error.to_string();
                        model_switch_error = Some(message.clone());
                        let _ = request.response.send(Err(message)).await;
                    }
                }
            }
            AgentCommand::UserTurn(input) => {
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
                let roster = roster.clone();
                spawn_turn(
                    &turn_runtime,
                    input,
                    context,
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
                    canvas.clone(),
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
                    canvas.clone(),
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
}

fn spawn_turn(
    runtime: &TurnRuntime,
    input: String,
    canvas: Option<agent::CurrentCanvasContext>,
    roster: Vec<agent::AgentRosterEntry>,
    response: async_channel::Sender<Result<Vec<AgentEvent>, String>>,
    turn_generation: u64,
) {
    let runtime = runtime.clone();
    shared_tokio_runtime()
        .expect("Norma Tokio runtime was initialized before spawning a turn")
        .spawn(async move {
            let mut session = runtime.session.lock().await;
            if runtime.generation.load(Ordering::Acquire) != turn_generation {
                let _ = response
                    .send(Err("Norma 回合已被新输入取代".to_string()))
                    .await;
                return;
            }
            if let Some(canvas) = canvas {
                session.update_environment(canvas, roster);
            }
            let cancel = session.cancel_handle();
            let dialogue_cancelled = Arc::clone(&runtime.dialogue_cancelled);
            let watcher = tokio::spawn(async move {
                while !dialogue_cancelled.load(Ordering::Acquire) {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                cancel.cancel();
            });
            let (delta_tx, mut delta_rx) = tokio::sync::mpsc::channel(32);
            let produced_deltas = Arc::new(Mutex::new(Vec::new()));
            let collected_deltas = Arc::clone(&produced_deltas);
            let live_events = runtime.events.clone();
            let delta_forwarder = tokio::spawn(async move {
                while let Some(text) = delta_rx.recv().await {
                    let event = AgentEvent::DialogueTextDelta { text };
                    collected_deltas
                        .lock()
                        .expect("Norma streamed event lock should not be poisoned")
                        .push(event.clone());
                    let _ = live_events.send(event).await;
                }
            });
            let result = session.turn_streaming(&input, delta_tx.clone()).await;
            drop(delta_tx);
            let _ = delta_forwarder.await;
            watcher.abort();
            match result {
                Ok(_result) => {
                    let mut produced = produced_deltas
                        .lock()
                        .expect("Norma streamed event lock should not be poisoned")
                        .clone();
                    let finished = AgentEvent::DialogueStreamFinished;
                    let _ = runtime.events.send(finished.clone()).await;
                    produced.push(finished);
                    let _ = response.send(Ok(produced)).await;
                }
                Err(error) => {
                    let message = error.to_string();
                    let _ = runtime
                        .events
                        .send(AgentEvent::ProviderError {
                            message: message.clone(),
                        })
                        .await;
                    let _ = response.send(Err(message)).await;
                }
            }
        });
}

impl MultisoulApp {
    pub(crate) fn submit_project_agent(
        &mut self,
        command: AgentCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<async_channel::Receiver<Result<Vec<AgentEvent>, String>>, String> {
        let config = self
            .voice_settings
            .values()
            .agent_provider_config()
            .map_err(|error| error.to_string())?;
        let root = self
            .workspace_state
            .current_root
            .as_ref()
            .ok_or_else(|| "使用 Norma 前请先打开项目".to_string())?;
        let project_id = root.display().to_string();
        let must_replace = self
            .agent_runtime
            .as_ref()
            .is_none_or(|runtime| !runtime.matches_project(&project_id));
        if must_replace {
            let (runtime, adapter, events) = ProjectAgentRuntime::spawn(project_id, config)?;
            let runtime_id = runtime.id();
            self.dialogue
                .set_dialogue_stream_cancellation(runtime.dialogue_cancel_handle());
            self.agent_runtime = Some(runtime);
            start_adapter_pump(adapter, runtime_id, window, cx);
            start_event_pump(events, runtime_id, cx);
        } else if self
            .agent_runtime
            .as_ref()
            .is_some_and(|runtime| !runtime.uses_config(&config))
        {
            let runtime = self
                .agent_runtime
                .as_ref()
                .expect("Norma runtime should exist while switching models");
            let _ = runtime.submit(AgentCommand::SwitchModel(config));
        }
        let roster = self.agent_roster_for_provider();
        let canvas = self.current_canvas_context_for_provider();
        let mut commands = contextualized_agent_commands(roster, canvas, command);
        let requested = commands
            .pop()
            .expect("contextualized command batch should include the requested command");
        let runtime = self
            .agent_runtime
            .as_ref()
            .expect("Norma runtime should exist after initialization");
        for context in commands {
            let _ = runtime.submit(context);
        }
        Ok(runtime.submit(requested))
    }
}

include!("agent_runtime/adapter_pump.rs");

#[cfg(test)]
#[path = "agent_runtime/tests.rs"]
mod tests;
