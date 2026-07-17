use crate::app::MultisoulApp;
use agent::{AgentCommand, AgentEvent, CardTaskJournal};
use gpui::{Context, Window};
use norma::protocol::{CardCommand, CardCommandResult};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

mod artifacts;
mod commands;
mod coordination;
mod diagnostics;
pub(super) mod pending;

pub(crate) use artifacts::artifacts_from_agent_events;
#[cfg(any(test, feature = "e2e-test-support"))]
pub(crate) use commands::execute_agent_card_command;
#[cfg(test)]
use coordination::agent_completion_report;

static NEXT_RUNTIME_ID: AtomicU64 = AtomicU64::new(1);

type ProjectAgentSystem = agent::AgentSystem<
    agent::ConfiguredProvider,
    agent::ConfiguredProvider,
    ChannelCardToolHost,
    ProjectTaskJournal,
>;

struct AgentRequest {
    command: AgentCommand,
    response: async_channel::Sender<Result<Vec<AgentEvent>, String>>,
}

struct AgentToolRequest {
    task_id: u64,
    tool_call_id: String,
    command: CardCommand,
    result_tx: mpsc::Sender<Result<CardCommandResult, String>>,
}

struct ProjectAgentWorker {
    project_id: String,
    project_db_path: Option<std::path::PathBuf>,
    config: agent::ProviderConfig,
    tool_tx: async_channel::Sender<AgentToolRequest>,
    requests: async_channel::Receiver<AgentRequest>,
    events: async_channel::Sender<AgentEvent>,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    dialogue_cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl AgentToolRequest {
    fn complete(self, result: Result<CardCommandResult, String>) {
        let _ = self.result_tx.send(result);
    }
}

struct ChannelCardToolHost {
    sender: async_channel::Sender<AgentToolRequest>,
}

impl agent::CardToolHost for ChannelCardToolHost {
    fn execute(
        &mut self,
        request: agent::CardToolRequest,
    ) -> Result<agent::CardToolResult, agent::CardToolError> {
        diagnostics::tool_bridge_send(&request);
        let (result_tx, result_rx) = mpsc::channel();
        self.sender
            .send_blocking(AgentToolRequest {
                task_id: request.task_id,
                tool_call_id: request.tool_call_id,
                command: request.command,
                result_tx,
            })
            .map_err(|_| agent::CardToolError::Execution("agent tool bridge closed".into()))?;
        let result = result_rx
            .recv()
            .map_err(|_| agent::CardToolError::Execution("agent tool result closed".into()))?
            .map_err(agent::CardToolError::Execution)?;
        Ok(agent::CardToolResult { result })
    }
}

enum ProjectTaskJournalBackend {
    Memory(agent::MemoryJournal),
    Persistent(storage::BackgroundAgentTaskJournal),
}

struct ProjectTaskJournal {
    backend: ProjectTaskJournalBackend,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl CardTaskJournal for ProjectTaskJournal {
    fn append(&mut self, mut entry: agent::AuditEntry) {
        if self.cancelled.load(Ordering::Acquire) {
            if let agent::AuditEntry::Task(record) = &mut entry {
                record.state = agent::TaskState::Interrupted;
                record.summary = None;
            }
        }
        match &mut self.backend {
            ProjectTaskJournalBackend::Memory(journal) => journal.append(entry),
            ProjectTaskJournalBackend::Persistent(journal) => journal.append(entry),
        }
    }

    fn recover_incomplete(&mut self, project_id: &str) -> Vec<u64> {
        match &mut self.backend {
            ProjectTaskJournalBackend::Memory(journal) => journal.recover_incomplete(project_id),
            ProjectTaskJournalBackend::Persistent(journal) => {
                journal.recover_incomplete(project_id)
            }
        }
    }

    fn next_task_id(&self, project_id: &str) -> u64 {
        match &self.backend {
            ProjectTaskJournalBackend::Memory(journal) => journal.next_task_id(project_id),
            ProjectTaskJournalBackend::Persistent(journal) => journal.next_task_id(project_id),
        }
    }
}

pub(crate) struct ProjectAgentRuntime {
    id: u64,
    project_id: String,
    config: agent::ProviderConfig,
    request_tx: async_channel::Sender<AgentRequest>,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    dialogue_cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl ProjectAgentRuntime {
    fn spawn(
        project_id: String,
        project_db_path: Option<std::path::PathBuf>,
        config: agent::ProviderConfig,
    ) -> Result<
        (
            Self,
            async_channel::Receiver<AgentToolRequest>,
            async_channel::Receiver<AgentEvent>,
        ),
        String,
    > {
        config.validate().map_err(|error| error.to_string())?;
        let id = NEXT_RUNTIME_ID.fetch_add(1, Ordering::Relaxed);
        diagnostics::runtime_spawn(id, &config, project_db_path.is_some());
        let (request_tx, request_rx) = async_channel::unbounded();
        let (tool_tx, tool_rx) = async_channel::bounded(1);
        let (event_tx, event_rx) = async_channel::unbounded();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let dialogue_cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let worker_dialogue_cancelled = dialogue_cancelled.clone();
        let worker_project_id = project_id.clone();
        let worker_config = config.clone();
        std::thread::Builder::new()
            .name("norma-project-agent".to_string())
            .spawn(move || {
                run_project_agent(ProjectAgentWorker {
                    project_id: worker_project_id,
                    project_db_path,
                    config: worker_config,
                    tool_tx,
                    requests: request_rx,
                    events: event_tx,
                    cancelled: worker_cancelled,
                    dialogue_cancelled: worker_dialogue_cancelled,
                })
            })
            .map_err(|error| format!("could not start project agent: {error}"))?;
        Ok((
            Self {
                id,
                project_id,
                config,
                request_tx,
                cancelled,
                dialogue_cancelled,
            },
            tool_rx,
            event_rx,
        ))
    }

    fn matches(&self, project_id: &str, config: &agent::ProviderConfig) -> bool {
        self.project_id == project_id && &self.config == config
    }

    fn id(&self) -> u64 {
        self.id
    }

    fn dialogue_cancel_handle(&self) -> Arc<std::sync::atomic::AtomicBool> {
        Arc::clone(&self.dialogue_cancelled)
    }

    pub(crate) fn submit(
        &self,
        command: AgentCommand,
    ) -> async_channel::Receiver<Result<Vec<AgentEvent>, String>> {
        diagnostics::command_submit(self.id, &command);
        let (response_tx, response_rx) = async_channel::bounded(1);
        let result = self.request_tx.try_send(AgentRequest {
            command,
            response: response_tx.clone(),
        });
        if result.is_err() {
            let _ = response_tx.try_send(Err("project agent is unavailable".to_string()));
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

fn run_project_agent(worker: ProjectAgentWorker) {
    let ProjectAgentWorker {
        project_id,
        project_db_path,
        config,
        tool_tx,
        requests,
        events,
        cancelled,
        dialogue_cancelled,
    } = worker;
    let dialogue = match agent::ConfiguredProvider::new(config.clone()) {
        Ok(provider) => provider,
        Err(error) => return reject_startup(requests, events, error.to_string()),
    };
    let card = match agent::ConfiguredProvider::new(config) {
        Ok(provider) => provider,
        Err(error) => return reject_startup(requests, events, error.to_string()),
    };
    let backend = match project_db_path {
        Some(path) => ProjectTaskJournalBackend::Persistent(
            storage::BackgroundAgentTaskJournal::new(storage::AgentTaskStore::new(path)),
        ),
        None => ProjectTaskJournalBackend::Memory(agent::MemoryJournal::default()),
    };
    let journal = ProjectTaskJournal {
        backend,
        cancelled: cancelled.clone(),
    };
    let live_events = events.clone();
    let mut system = agent::AgentSystem::new(
        project_id,
        dialogue,
        card,
        ChannelCardToolHost { sender: tool_tx },
        journal,
        agent::CardTaskBudget::default(),
    )
    .with_live_event_sink(move |event| {
        let _ = live_events.send_blocking(event);
    })
    .with_live_dialogue_cancellation(Arc::clone(&dialogue_cancelled));
    publish_events(system.events().to_vec(), &events);
    while let Ok(request) = requests.recv_blocking() {
        diagnostics::worker_command(&request.command, system.pending_task_count());
        let closing = matches!(request.command, AgentCommand::ProjectClosed);
        handle_request(&mut system, request, &events, &dialogue_cancelled);
        if closing {
            break;
        }
        while system.pending_task_count() > 0 {
            if interrupt_cancelled(&mut system, &events, &cancelled) {
                return;
            }
            while let Ok(request) = requests.try_recv() {
                handle_request(&mut system, request, &events, &dialogue_cancelled);
            }
            let produced = system.pump();
            diagnostics::worker_pump(&produced, system.pending_task_count());
            if interrupt_cancelled(&mut system, &events, &cancelled) {
                return;
            }
            publish_events(produced, &events);
        }
    }
}

fn interrupt_cancelled(
    system: &mut ProjectAgentSystem,
    events: &async_channel::Sender<AgentEvent>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> bool {
    if !cancelled.load(Ordering::Acquire) {
        return false;
    }
    if let Ok(produced) = system.submit(AgentCommand::ProjectClosed) {
        publish_events(produced, events);
    }
    true
}

fn reject_startup(
    requests: async_channel::Receiver<AgentRequest>,
    events: async_channel::Sender<AgentEvent>,
    message: String,
) {
    let _ = events.send_blocking(AgentEvent::ProviderError {
        message: message.clone(),
    });
    while let Ok(request) = requests.recv_blocking() {
        let _ = request.response.send_blocking(Err(message.clone()));
    }
}

fn handle_request(
    system: &mut ProjectAgentSystem,
    request: AgentRequest,
    events: &async_channel::Sender<AgentEvent>,
    dialogue_cancelled: &std::sync::atomic::AtomicBool,
) {
    let is_user_turn = matches!(request.command, AgentCommand::UserTurn(_));
    if is_user_turn {
        dialogue_cancelled.store(false, Ordering::Release);
    }
    let result = system
        .submit(request.command)
        .map_err(|error| error.to_string());
    diagnostics::worker_result(&result, system.pending_task_count());
    if let Ok(produced) = &result {
        if is_user_turn {
            let had_live_dialogue_text = produced
                .iter()
                .any(|event| matches!(event, AgentEvent::DialogueTextDelta { .. }));
            if had_live_dialogue_text {
                let _ = events.send_blocking(AgentEvent::DialogueStreamFinished);
            }
        } else {
            publish_events(produced.clone(), events);
        }
    }
    let _ = request.response.send_blocking(result);
}

fn publish_events(produced: Vec<AgentEvent>, events: &async_channel::Sender<AgentEvent>) {
    for event in produced {
        let _ = events.send_blocking(event);
    }
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
            .ok_or_else(|| "open a project before using the agent".to_string())?;
        let project_id = root.display().to_string();
        let must_replace = self
            .agent_runtime
            .as_ref()
            .is_none_or(|runtime| !runtime.matches(&project_id, &config));
        if must_replace {
            let project_db_path = Some(root.join(".multisoul").join("project.sqlite"));
            let (runtime, tools, events) =
                ProjectAgentRuntime::spawn(project_id, project_db_path, config)?;
            let runtime_id = runtime.id();
            let dialogue_cancel_handle = runtime.dialogue_cancel_handle();
            self.agent_runtime = Some(runtime);
            self.dialogue
                .set_dialogue_stream_cancellation(dialogue_cancel_handle);
            start_tool_pump(tools, runtime_id, window, cx);
            start_event_pump(events, runtime_id, cx);
        }
        let roster = self.agent_roster_for_provider();
        let runtime = self
            .agent_runtime
            .as_ref()
            .expect("agent runtime should exist after initialization");
        let _ = runtime.submit(AgentCommand::UpdateAgentRoster(roster));
        Ok(runtime.submit(command))
    }
}

fn start_tool_pump(
    tools: async_channel::Receiver<AgentToolRequest>,
    runtime_id: u64,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    cx.spawn_in(window, async move |this, cx| {
        while let Ok(request) = tools.recv().await {
            diagnostics::tool_bridge_receive(runtime_id, &request);
            let command = request.command.clone();
            let result = this.update_in(cx, |app, window, _cx| {
                if !is_current_runtime(app, runtime_id) {
                    return Err("stale project agent runtime".to_string());
                }
                commands::execute_agent_card_command(app, window, command)
            });
            let result = match result {
                Ok(result) => result,
                Err(_) => Err("agent tool app context closed".to_string()),
            };
            diagnostics::tool_bridge_complete(runtime_id, &request, result.is_ok());
            request.complete(result);
        }
    })
    .detach();
}

fn start_event_pump(
    events: async_channel::Receiver<AgentEvent>,
    runtime_id: u64,
    cx: &mut Context<MultisoulApp>,
) {
    cx.spawn(async move |this, cx| {
        while let Ok(event) = events.recv().await {
            diagnostics::event_receive(runtime_id, &event);
            let _ = this.update(cx, |app, cx| {
                if !is_current_runtime(app, runtime_id) {
                    return;
                }
                match event {
                    AgentEvent::DialogueTextDelta { text } => {
                        app.dialogue.on_agent_text_delta(&text, cx);
                    }
                    AgentEvent::DialogueStreamFinished => {
                        app.dialogue.finish_agent_text_stream(cx);
                    }
                    _ => {
                        if let Some(report) = terminal_task_report(&event) {
                            app.dialogue.queue_agent_report(report);
                        }
                    }
                }
                cx.notify();
            });
        }
    })
    .detach();
}

fn terminal_task_report(event: &AgentEvent) -> Option<String> {
    match event {
        AgentEvent::TaskSucceeded { summary, .. } => Some(summary.clone()),
        AgentEvent::TaskFailed { reason, .. } => Some(format!("任务失败：{reason}")),
        AgentEvent::TaskCancelled {
            partial_effects: true,
            ..
        } => Some("任务已取消；已发生的操作不会回滚".to_string()),
        AgentEvent::TaskCancelled { .. } => Some("任务已取消".to_string()),
        AgentEvent::TaskInterrupted { .. } => Some("任务已中断".to_string()),
        _ => None,
    }
}

fn is_current_runtime(app: &MultisoulApp, runtime_id: u64) -> bool {
    app.agent_runtime
        .as_ref()
        .is_some_and(|runtime| runtime.id() == runtime_id)
}

#[cfg(test)]
#[path = "../../tests/unit/agent_runtime_tests.rs"]
mod agent_runtime_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent_dispatch_submit_tests.rs"]
mod agent_dispatch_submit_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent_dispatch_workspace_tests.rs"]
mod agent_dispatch_workspace_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent_dispatch_queue_failure_tests.rs"]
mod agent_dispatch_queue_failure_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent_dispatch_unsafe_failure_tests.rs"]
mod agent_dispatch_unsafe_failure_tests;
