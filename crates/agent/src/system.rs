use crate::journal::{AuditEntry, CardTaskJournal};
use crate::provider::{assistant_message, tool_message, user_message, AgentProvider};
use crate::tools::{parse_card_subagent_tool_call, parse_dialogue_tool_call, DialogueToolAction};
use crate::{
    dialogue_tool_specs, AgentCommand, AgentError, AgentEvent, AgentMessage, CardTaskBudget,
    CardToolHost, CardToolRequest, CardToolResult, ProviderRequest, ProviderToolCall, TaskId,
    TaskRecord, TaskState, ToolCallRecord, ToolErrorRecord,
};
use norma::protocol::CardCommandResult;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
pub(crate) struct PendingTask {
    pub(crate) task_id: TaskId,
    intent: String,
}
pub(crate) struct ActiveTask {
    pub(crate) task_id: TaskId,
    intent: String,
    messages: Vec<AgentMessage>,
    pending_tool_calls: VecDeque<ProviderToolCall>,
    cached_results: HashMap<String, Result<CardCommandResult, String>>,
    round_count: usize,
    tool_call_count: usize,
    consecutive_failures: usize,
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
    dialogue_messages: Vec<AgentMessage>,
    pub(crate) pending: VecDeque<PendingTask>,
    pub(crate) active: Option<ActiveTask>,
    pub(crate) records: HashMap<TaskId, TaskRecord>,
    next_task_id: TaskId,
    events: Vec<AgentEvent>,
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
        journal: J,
        budget: CardTaskBudget,
    ) -> Self {
        let project_id = project_id.into();
        let next_task_id = journal.next_task_id(&project_id);
        Self {
            project_id,
            dialogue_provider,
            card_provider,
            card_tool_host,
            journal,
            budget,
            dialogue_messages: Vec::new(),
            pending: VecDeque::new(),
            active: None,
            records: HashMap::new(),
            next_task_id,
            events: Vec::new(),
        }
    }

    pub fn submit(&mut self, command: AgentCommand) -> Result<Vec<AgentEvent>, AgentError> {
        let start = self.events.len();
        match command {
            AgentCommand::UserTurn(text) => self.handle_user_turn(text)?,
            AgentCommand::DirectCardTask(intent) => {
                self.enqueue_task(intent);
            }
            AgentCommand::CancelLatest => self.cancel_latest()?,
            AgentCommand::CancelTask { task_id } => self.cancel_task(task_id)?,
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
            let intent = task.intent;
            self.update_task_state(task_id, TaskState::Running, None, None);
            self.emit_event(AgentEvent::TaskRunning { task_id });
            self.active = Some(ActiveTask {
                task_id,
                intent,
                messages: vec![user_message(self.task_intent(task_id))],
                pending_tool_calls: VecDeque::new(),
                cached_results: HashMap::new(),
                round_count: 0,
                tool_call_count: 0,
                consecutive_failures: 0,
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

    fn handle_user_turn(&mut self, text: String) -> Result<(), AgentError> {
        if text.trim().is_empty() {
            return Err(AgentError::InvalidCommand(
                "user turn must not be empty".to_string(),
            ));
        }
        tracing::debug!(project_id = %self.project_id, "dialogue user turn received");
        self.dialogue_messages.push(user_message(&text));
        let request = ProviderRequest {
            messages: self.dialogue_messages.clone(),
            tools: dialogue_tool_specs(),
        };
        let mut streamed = Vec::new();
        let response = self.dialogue_provider.stream(&request, &mut |event| {
            if let crate::ProviderStreamEvent::TextDelta(text) = event {
                if !text.is_empty() {
                    streamed.push(text);
                }
            }
        })?;
        let actions = response
            .tool_calls
            .iter()
            .filter(|call| call.name == crate::CARD_TASK_TOOL)
            .map(|call| {
                parse_dialogue_tool_call(&call.arguments_json)
                    .map_err(|error| AgentError::InvalidCommand(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let has_task_start = actions
            .iter()
            .any(|action| matches!(action, DialogueToolAction::Start { .. }));
        if !has_task_start {
            for text in streamed {
                self.emit_event(AgentEvent::DialogueTextDelta { text });
            }
        }
        self.dialogue_messages
            .push(assistant_message(&response.text, &response.tool_calls));
        let mut started = false;
        for action in actions {
            match action {
                DialogueToolAction::Start { intent } => {
                    if !started {
                        self.enqueue_task(intent);
                        started = true;
                    }
                }
                DialogueToolAction::Cancel { task_id } => self.cancel_task(task_id)?,
                DialogueToolAction::Status { task_id } => self.emit_status(task_id),
            }
        }
        Ok(())
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
        self.pending.push_back(PendingTask {
            task_id,
            intent: intent.clone(),
        });
        tracing::info!(
            project_id = %self.project_id,
            task_id,
            intent = %intent,
            "card task queued by dialogue"
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
        tracing::debug!(
            project_id = %self.project_id,
            task_id = active.task_id,
            round = active.round_count,
            "card subagent provider round started"
        );
        let request = ProviderRequest {
            messages: active.messages.clone(),
            tools: crate::card_subagent_tool_specs(),
        };
        let response = match self.card_provider.complete(&request) {
            Ok(response) => response,
            Err(error) => {
                self.finish_failed(error.to_string());
                return;
            }
        };
        if response.tool_calls.is_empty() {
            let task_id = active.task_id;
            let summary = if response.text.trim().is_empty() {
                active.intent.clone()
            } else {
                response.text
            };
            tracing::info!(
                project_id = %self.project_id,
                task_id,
                "card subagent completed task"
            );
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
                    let result = self
                        .card_tool_host
                        .execute(request.clone())
                        .map(|value: CardToolResult| value.result)
                        .map_err(|error| error.to_string());
                    self.journal.append(AuditEntry::ToolCall(ToolCallRecord {
                        project_id: self.project_id.clone(),
                        task_id: active.task_id,
                        tool_call_id: call.id.clone(),
                        command: request.command,
                        result: result.clone(),
                    }));
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
            }
            Err(_) => active.consecutive_failures += 1,
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
