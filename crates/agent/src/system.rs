use crate::journal::{AuditEntry, CardTaskJournal};
use crate::provider::{
    assistant_message, system_message, tool_message, user_message, AgentProvider,
};
use crate::tools::parse_card_subagent_tool_call;
use crate::{
    AgentCommand, AgentError, AgentEvent, AgentMessage, CardTaskBudget, CardToolHost,
    CardToolRequest, CardToolResult, ProviderRequest, ProviderToolCall, TaskId, TaskRecord,
    TaskState, ToolCallRecord, ToolErrorRecord,
};
use norma::protocol::{CardCommand, CardCommandResult};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

pub(crate) mod card_task;
mod dialogue;
mod dialogue_policy;
mod roster;
pub(crate) struct PendingTask {
    pub(crate) task_id: TaskId,
}
struct UnresolvedToolFailure {
    command: Option<CardCommand>,
    error: String,
}
pub(crate) struct ActiveTask {
    pub(crate) task_id: TaskId,
    messages: Vec<AgentMessage>,
    pending_tool_calls: VecDeque<ProviderToolCall>,
    cached_results: HashMap<String, Result<CardCommandResult, String>>,
    created_node_ids: HashSet<u64>,
    verified_node_ids: HashSet<u64>,
    successful_results: Vec<CardCommandResult>,
    round_count: usize,
    tool_call_count: usize,
    consecutive_failures: usize,
    unresolved_tool_failures: Vec<UnresolvedToolFailure>,
    pub(crate) completed_tool_calls: usize,
    pub(crate) cancel_requested: bool,
    started_at: Instant,
}
pub struct AgentSystem<D, C, H, J>
where
    D: AgentProvider,
    C: AgentProvider,
    H: CardToolHost,
    J: CardTaskJournal,
{
    project_id: String,
    dialogue_provider: D,
    card_provider: C,
    card_tool_host: H,
    journal: J,
    budget: CardTaskBudget,
    dialogue_turns: VecDeque<Vec<AgentMessage>>,
    pub(crate) pending: VecDeque<PendingTask>,
    pub(crate) active: Option<ActiveTask>,
    pub(crate) records: HashMap<TaskId, TaskRecord>,
    next_task_id: TaskId,
    events: Vec<AgentEvent>,
    live_event_sink: Option<Arc<dyn Fn(AgentEvent) + Send + Sync>>,
    live_dialogue_cancellation: Option<Arc<AtomicBool>>,
    agent_roster: Vec<crate::AgentRosterEntry>,
    agent_roster_prompt: String,
}
impl<D, C, H, J> AgentSystem<D, C, H, J>
where
    D: AgentProvider,
    C: AgentProvider,
    H: CardToolHost,
    J: CardTaskJournal,
{
    pub fn new(
        project_id: impl Into<String>,
        dialogue_provider: D,
        card_provider: C,
        card_tool_host: H,
        mut journal: J,
        budget: CardTaskBudget,
    ) -> Self {
        let project_id = project_id.into();
        let interrupted = journal.recover_incomplete(&project_id);
        let next_task_id = journal.next_task_id(&project_id);
        Self {
            project_id,
            dialogue_provider,
            card_provider,
            card_tool_host,
            journal,
            budget,
            dialogue_turns: VecDeque::new(),
            pending: VecDeque::new(),
            active: None,
            records: HashMap::new(),
            next_task_id,
            events: interrupted
                .into_iter()
                .map(|task_id| AgentEvent::TaskInterrupted { task_id })
                .collect(),
            live_event_sink: None,
            live_dialogue_cancellation: None,
            agent_roster: Vec::new(),
            agent_roster_prompt: String::new(),
        }
    }

    /// Installs an observer for ordinary dialogue deltas while a provider is streaming.
    pub fn with_live_event_sink(
        mut self,
        sink: impl Fn(AgentEvent) + Send + Sync + 'static,
    ) -> Self {
        self.live_event_sink = Some(Arc::new(sink));
        self
    }

    /// Installs the cancellation flag controlled by the app's barge-in policy.
    pub fn with_live_dialogue_cancellation(mut self, cancellation: Arc<AtomicBool>) -> Self {
        self.live_dialogue_cancellation = Some(cancellation);
        self
    }

    pub fn submit(&mut self, command: AgentCommand) -> Result<Vec<AgentEvent>, AgentError> {
        let start = self.events.len();
        match command {
            AgentCommand::UpdateAgentRoster(roster) => self
                .set_agent_roster(roster)
                .map_err(|error| AgentError::InvalidCommand(error.to_string()))?,
            AgentCommand::UserTurn(text) => self.handle_user_turn(text)?,
            AgentCommand::DirectCardTask(intent) => {
                self.enqueue_task(intent);
            }
            AgentCommand::CancelLatest => self.cancel_latest()?,
            AgentCommand::CancelTask { task_id } => {
                self.cancel_task(task_id)?;
            }
            AgentCommand::Status { task_id } => self.emit_status(task_id),
            AgentCommand::CardActivity { node_id, summary } => {
                self.emit_event(AgentEvent::CardActivityNotification { node_id, summary });
            }
            AgentCommand::ProjectClosed => self.interrupt_for_project_close(),
        }
        Ok(self.events[start..].to_vec())
    }

    pub fn pump(&mut self) -> Vec<AgentEvent> {
        let start = self.events.len();
        if self.active.is_none() {
            let Some(task) = self.pending.pop_front() else {
                return Vec::new();
            };
            let task_id = task.task_id;
            self.update_task_state(task_id, TaskState::Running, None, None);
            self.emit_event(AgentEvent::TaskRunning { task_id });
            self.active = Some(ActiveTask {
                task_id,
                messages: vec![
                    system_message(crate::card_task_system_prompt_with_roster(
                        &self.agent_roster_prompt,
                    )),
                    user_message(self.task_intent(task_id)),
                ],
                pending_tool_calls: VecDeque::new(),
                cached_results: HashMap::new(),
                created_node_ids: HashSet::new(),
                verified_node_ids: HashSet::new(),
                successful_results: Vec::new(),
                round_count: 0,
                tool_call_count: 0,
                consecutive_failures: 0,
                unresolved_tool_failures: Vec::new(),
                completed_tool_calls: 0,
                cancel_requested: false,
                started_at: Instant::now(),
            });
            self.request_next_card_response();
            return self.events[start..].to_vec();
        }

        if self
            .active
            .as_ref()
            .is_some_and(|task| task.cancel_requested)
        {
            self.finish_cancelled();
            return self.events[start..].to_vec();
        }

        if self
            .active
            .as_ref()
            .is_some_and(|task| task.started_at.elapsed() >= self.budget.max_wall_time)
        {
            self.finish_failed("BudgetExceeded: wall-clock time".to_string());
            return self.events[start..].to_vec();
        }

        if self.active.as_ref().is_some_and(|task| {
            task.pending_tool_calls.is_empty() && task.round_count >= self.budget.max_model_rounds
        }) {
            self.finish_failed("BudgetExceeded: model rounds".to_string());
            return self.events[start..].to_vec();
        }

        if self
            .active
            .as_ref()
            .is_some_and(|task| task.pending_tool_calls.is_empty())
        {
            self.request_next_card_response();
            return self.events[start..].to_vec();
        }

        self.execute_next_tool_call();
        self.events[start..].to_vec()
    }

    pub fn pending_task_count(&self) -> usize {
        self.pending.len() + usize::from(self.active.is_some())
    }

    pub fn events(&self) -> &[AgentEvent] {
        &self.events
    }

    pub fn card_tool_host(&self) -> &H {
        &self.card_tool_host
    }

    pub fn card_tool_host_mut(&mut self) -> &mut H {
        &mut self.card_tool_host
    }

    pub fn journal(&self) -> &J {
        &self.journal
    }

    pub fn journal_mut(&mut self) -> &mut J {
        &mut self.journal
    }

    pub fn task_record(&self, task_id: TaskId) -> Option<&TaskRecord> {
        self.records.get(&task_id)
    }

    fn enqueue_task(&mut self, intent: String) -> TaskId {
        let task_id = self.next_task_id;
        self.next_task_id += 1;
        let record = TaskRecord {
            project_id: self.project_id.clone(),
            task_id,
            intent: intent.clone(),
            state: TaskState::Queued,
            summary: None,
            error: None,
            completed_tool_calls: 0,
        };
        self.journal.append(AuditEntry::Task(record.clone()));
        self.records.insert(task_id, record);
        self.pending.push_back(PendingTask { task_id });
        tracing::info!(
            project_id = %self.project_id,
            task_id,
            intent_chars = intent.chars().count(),
            "[DEBUG-agent-flow-7c1e] card task queued by dialogue"
        );
        self.emit_event(AgentEvent::TaskQueued {
            task_id,
            summary: intent,
        });
        task_id
    }

    fn request_next_card_response(&mut self) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        active.round_count += 1;
        let request = ProviderRequest {
            messages: active.messages.clone(),
            tools: crate::card_subagent_tool_specs(),
        };
        crate::diagnostics::card_request(
            &self.project_id,
            active.task_id,
            active.round_count,
            &request,
        );
        let response = match self.card_provider.complete(&request) {
            Ok(response) => response,
            Err(error) => {
                self.finish_failed(error.to_string());
                return;
            }
        };
        crate::diagnostics::card_response(
            &self.project_id,
            active.task_id,
            active.round_count,
            active.completed_tool_calls,
            &response,
        );
        if response.tool_calls.is_empty() {
            let task_id = active.task_id;
            if active.completed_tool_calls == 0 {
                self.finish_failed("TaskIncomplete: no successful card-tool receipt".to_string());
                return;
            }
            if !active.unresolved_tool_failures.is_empty() {
                let errors = active
                    .unresolved_tool_failures
                    .iter()
                    .map(|failure| failure.error.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                self.finish_failed(format!(
                    "TaskIncomplete: unresolved card-tool failures: {errors}"
                ));
                return;
            }
            let summary = card_task::successful_summary(active);
            self.update_task_state(task_id, TaskState::Succeeded, Some(summary.clone()), None);
            self.emit_event(AgentEvent::TaskSucceeded { task_id, summary });
            self.active = None;
            return;
        }
        active
            .messages
            .push(assistant_message(&response.text, &response.tool_calls));
        active.pending_tool_calls.extend(response.tool_calls);
    }

    fn execute_next_tool_call(&mut self) {
        let Some(mut active) = self.active.take() else {
            return;
        };
        let Some(call) = active.pending_tool_calls.pop_front() else {
            self.active = Some(active);
            return;
        };
        if active.tool_call_count >= self.budget.max_tool_calls {
            self.active = Some(active);
            self.finish_failed("BudgetExceeded: tool calls".to_string());
            return;
        }
        self.emit_event(AgentEvent::ToolStarted {
            task_id: active.task_id,
            tool_call_id: call.id.clone(),
        });
        tracing::info!(
            project_id = %self.project_id,
            task_id = active.task_id,
            tool_call_id = %call.id,
            tool = %call.name,
            "card subagent tool call started"
        );
        let mut attempted_command = None;
        let is_cached = active.cached_results.contains_key(&call.id);
        let result = if let Some(result) = active.cached_results.get(&call.id) {
            result.clone()
        } else {
            active.tool_call_count += 1;
            match parse_card_subagent_tool_call(&call.name, &call.arguments_json) {
                Ok(command) => {
                    let request = CardToolRequest {
                        project_id: self.project_id.clone(),
                        task_id: active.task_id,
                        tool_call_id: call.id.clone(),
                        command,
                    };
                    attempted_command = Some(request.command.clone());
                    let result = card_task::require_verified_target(&active, &request.command)
                        .and_then(|()| {
                            self.card_tool_host
                                .execute(request.clone())
                                .map(|value: CardToolResult| value.result)
                                .map_err(|error| error.to_string())
                        });
                    self.journal.append(AuditEntry::ToolCall(ToolCallRecord {
                        project_id: self.project_id.clone(),
                        task_id: active.task_id,
                        tool_call_id: call.id.clone(),
                        command: request.command,
                        result: result.clone(),
                    }));
                    if let Ok(result) = &result {
                        card_task::record_successful_result(&mut active, result);
                    }
                    active
                        .cached_results
                        .insert(call.id.clone(), result.clone());
                    result
                }
                Err(error) => {
                    let error = error.to_string();
                    self.journal.append(AuditEntry::ToolError(ToolErrorRecord {
                        project_id: self.project_id.clone(),
                        task_id: active.task_id,
                        tool_call_id: call.id.clone(),
                        tool_name: call.name.clone(),
                        arguments_json: call.arguments_json.clone(),
                        error: error.clone(),
                    }));
                    active
                        .cached_results
                        .insert(call.id.clone(), Err(error.clone()));
                    Err(error)
                }
            }
        };
        match &result {
            Ok(_) => {
                active.completed_tool_calls += 1;
                active.consecutive_failures = 0;
                if let Some(command) = attempted_command.as_ref() {
                    active
                        .unresolved_tool_failures
                        .retain(|failure| failure.command.as_ref() != Some(command));
                }
            }
            Err(error) => {
                active.consecutive_failures += 1;
                if !is_cached {
                    active.unresolved_tool_failures.push(UnresolvedToolFailure {
                        command: attempted_command,
                        error: format!("{}: {error}", call.name),
                    });
                }
            }
        }
        active
            .messages
            .push(tool_message(call.id.clone(), card_result_message(&result)));
        self.emit_event(AgentEvent::ToolFinished {
            task_id: active.task_id,
            tool_call_id: call.id.clone(),
            result: result.clone(),
        });
        tracing::info!(
            project_id = %self.project_id,
            task_id = active.task_id,
            tool = %call.name,
            success = result.is_ok(),
            "card subagent tool call finished"
        );
        let task_id = active.task_id;
        self.update_completed_count(task_id, active.completed_tool_calls);
        if active.consecutive_failures >= self.budget.max_consecutive_tool_failures {
            self.active = Some(active);
            self.finish_failed("BudgetExceeded: consecutive tool failures".to_string());
        } else {
            self.active = Some(active);
        }
    }

    fn task_intent(&self, task_id: TaskId) -> String {
        self.records
            .get(&task_id)
            .map(|record| record.intent.clone())
            .unwrap_or_default()
    }

    fn update_completed_count(&mut self, task_id: TaskId, completed: usize) {
        if let Some(record) = self.records.get_mut(&task_id) {
            record.completed_tool_calls = completed;
            self.journal.append(AuditEntry::Task(record.clone()));
        }
    }

    pub(crate) fn update_task_state(
        &mut self,
        task_id: TaskId,
        state: TaskState,
        summary: Option<String>,
        error: Option<String>,
    ) {
        if let Some(record) = self.records.get_mut(&task_id) {
            record.state = state;
            record.summary = summary;
            record.error = error;
            self.journal.append(AuditEntry::Task(record.clone()));
        }
    }

    pub(crate) fn emit_event(&mut self, event: AgentEvent) {
        self.events.push(event);
    }
}
fn card_result_message(result: &Result<CardCommandResult, String>) -> String {
    match result {
        Ok(result) => serde_json::to_string(result).unwrap_or_else(|_| "ok".to_string()),
        Err(error) => format!("{{\"error\":{}}}", serde_json::json!(error)),
    }
}
