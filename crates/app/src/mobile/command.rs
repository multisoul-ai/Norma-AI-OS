use super::server::Shared;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use storage::{MobileCommandReservation, MobileCommandStore, NewMobileCommand};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MobileCommandRequest {
    pub operation_id: String,
    pub workspace_id: String,
    pub route_id: String,
    pub grant_revision: u64,
    pub command: MobileCommand,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanPriority {
    pub plan_id: String,
    pub todo_index: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MobileCommand {
    TerminalInput {
        expected: norma::mobile::CardSnapshot,
        screen_signature: u64,
        input_sequence: u64,
        input: norma::mobile::TerminalInput,
    },
    FeedbackDelivery {
        expected: norma::mobile::CardSnapshot,
        feedback: storage::DeliveryFeedback,
    },
    AcceptDelivery {
        key: storage::MobileTurnKey,
        fingerprint: String,
    },
    DecidePlanTodo {
        plan_id: String,
        todo_index: usize,
        choice: agent::PlanTodoChoice,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_of: Option<storage::TodoDecisionRef>,
    },
    GeneratePlan {
        direction: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        priority: Option<PlanPriority>,
    },
    CreateCard {
        profile: MobileProvider,
        title: Option<String>,
    },
    DeleteCard {
        expected: norma::mobile::CardSnapshot,
        confirm_busy: bool,
    },
    InterruptTurn {
        expected: norma::mobile::CardSnapshot,
    },
    DecideApproval {
        expected: norma::mobile::ApprovalTarget,
        decision: norma::agent_hook::ApprovalDecision,
    },
    QueueMessage {
        expected: norma::mobile::CardSnapshot,
        text: String,
    },
    SendMessage {
        expected: norma::mobile::CardSnapshot,
        text: String,
    },
}

impl MobileCommand {
    pub(crate) fn message_target(&self) -> Option<&norma::mobile::CardSnapshot> {
        match self {
            Self::SendMessage { expected, .. } | Self::FeedbackDelivery { expected, .. } => {
                Some(expected)
            }
            _ => None,
        }
    }
    pub(crate) fn validate_feedback_target(&self) -> Result<(), &'static str> {
        if let Self::FeedbackDelivery { expected, feedback } = self {
            feedback.validate().map_err(|_| "invalid_feedback")?;
            let key = &feedback.key;
            if expected.node_id != key.node_id
                || expected.profile != key.profile
                || expected.run_id.as_ref() != Some(&key.run_id)
                || expected.session_id.as_ref() != Some(&key.session_id)
            {
                return Err("session_changed");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
pub enum MobileProvider {
    Codex,
    Claude,
}

/// A single dispatch claim; revoke/shutdown competes with begin without locking GPUI.
pub struct CommandLease {
    state: AtomicU8,
    pub(super) operation_id: String,
    pub(super) finished: AtomicBool,
}
impl CommandLease {
    pub(super) fn new(operation: &str) -> Self {
        Self {
            state: AtomicU8::new(0),
            operation_id: operation.into(),
            finished: AtomicBool::new(false),
        }
    }
    pub fn begin(&self) -> bool {
        self.state
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub(super) fn cancel(&self) -> bool {
        self.state
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

pub struct LiveCommandRequest {
    pub source: NewMobileCommand,
    pub request: MobileCommandRequest,
    pub lease: Arc<CommandLease>,
    pub deadline: Instant,
    pub reply: crossbeam_channel::Sender<Result<Value, &'static str>>,
}

pub(super) fn submit(
    shared: &Arc<Shared>,
    machine: &str,
    device: &str,
    token: &str,
    request: MobileCommandRequest,
) -> Result<Value, &'static str> {
    validate(&request)?;
    shared.commands.as_ref().ok_or("unavailable")?;
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if !grant.allows(&request.workspace_id, true) {
        return Err("unauthorized");
    }
    if grant.revision != request.grant_revision {
        return Err("grant_changed");
    }
    let store = MobileCommandStore::new(access.store.path().to_path_buf());
    let record = NewMobileCommand {
        device_id: device.into(),
        operation_id: request.operation_id.clone(),
        workspace_id: request.workspace_id.clone(),
        route_id: request.route_id.clone(),
        owner_id: shared.owner_id.clone(),
        request_hash: hex::encode(Sha256::digest(
            serde_json::to_vec(&request).map_err(|_| "invalid_request")?,
        )),
    };
    if let Some(expected) = request.command.message_target() {
        if let Some(run) = &expected.run_id {
            if store
                .has_pending_queued_message(&record, expected.node_id, run)
                .map_err(|_| "unavailable")?
                && store
                    .get(
                        device,
                        &record.operation_id,
                        &record.workspace_id,
                        &shared.owner_id,
                    )
                    .map_err(|_| "unavailable")?
                    .is_none()
            {
                return Err("queue_pending");
            }
        }
    }
    if matches!(request.command, MobileCommand::DecidePlanTodo { .. }) {
        drop(access);
        return super::todo::submit(shared, machine, device, token, request, record);
    }
    if matches!(request.command, MobileCommand::QueueMessage { .. }) {
        drop(access);
        return super::queue::enqueue(shared, machine, device, token, request, record);
    }
    // At most eight command completions/foreground waits exist across all phones.
    let lease = shared.command_lease(device, &request.operation_id)?;
    match store.reserve(&record).map_err(|_| "unavailable")? {
        MobileCommandReservation::Conflict => return Err("idempotency_conflict"),
        MobileCommandReservation::Existing(receipt) => {
            // This lookup owns no execution. Do not count its temporary lease as pending work.
            lease.finished.store(true, Ordering::Release);
            return Ok(receipt_response(shared, *receipt));
        }
        MobileCommandReservation::New => {}
    }
    dispatch_reserved(shared, request, record, store, lease)
}

/// Both normal messages and confirmed Todos use this one bounded foreground dispatch/receipt path.
pub(super) fn dispatch_reserved(
    shared: &Arc<Shared>,
    request: MobileCommandRequest,
    record: NewMobileCommand,
    store: MobileCommandStore,
    lease: Arc<CommandLease>,
) -> Result<Value, &'static str> {
    let source = shared.commands.as_ref().ok_or("unavailable")?;
    let (reply, outcome) = crossbeam_channel::bounded(1);
    // Writes return an accepted receipt immediately; UI/persistence completion is queried separately.
    // Sends add10s task baseline capture to25s Provider acknowledgement and5s receipt margin.
    // Other commands keep30s; unknown outcomes are never replayed.
    let deadline = Instant::now()
        + Duration::from_secs(if request.command.message_target().is_some() {
            40
        } else {
            30
        });
    if source
        .try_send(LiveCommandRequest {
            source: record.clone(),
            request,
            lease: Arc::clone(&lease),
            deadline,
            reply,
        })
        .is_err()
    {
        lease.cancel();
        store
            .finish(&record, Err("unavailable".into()))
            .map_err(|_| "unavailable")?;
    } else {
        let completion_store = store.clone();
        let completion_record = record.clone();
        let cancel_on_failure = Arc::clone(&lease);
        let worker = std::thread::Builder::new().name("norma-mobile-command".into()).spawn(move || {
            let result = outcome.recv_timeout(deadline.saturating_duration_since(Instant::now())).unwrap_or_else(|_| {
                if lease.cancel() { Err("expired") } else { Err("outcome_unknown") }
            });
            if let Err(error) = completion_store.finish(&completion_record, result.map_err(str::to_owned)) {
                tracing::warn!(%error, operation_id=%completion_record.operation_id, "mobile command receipt persistence failed");
            }
            super::project_events::changed();
            lease.finished.store(true, Ordering::Release);
        });
        if worker.is_err() {
            cancel_on_failure.cancel();
            cancel_on_failure.finished.store(true, Ordering::Release);
            // Execution might already have begun; never turn this into a retryable fresh command.
            store
                .finish(&record, Err("outcome_unknown".into()))
                .map_err(|_| "unavailable")?;
        }
    }
    super::project_events::changed();
    let receipt = store
        .get(
            &record.device_id,
            &record.operation_id,
            &record.workspace_id,
            &shared.owner_id,
        )
        .map_err(|_| "unavailable")?
        .ok_or("unavailable")?;
    Ok(receipt_response(shared, receipt))
}

pub(super) fn receipt(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    workspace: &str,
    operation: &str,
) -> Result<Value, &'static str> {
    if !valid_id(workspace) || !valid_id(operation) {
        return Err("invalid_request");
    }
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    if !grant.allows(workspace, false) {
        return Err("unauthorized");
    }
    let receipt = MobileCommandStore::new(access.store.path().to_path_buf())
        .get(device, operation, workspace, &shared.owner_id)
        .map_err(|_| "unavailable")?
        .ok_or("not_found")?;
    Ok(receipt_response(shared, receipt))
}

pub(super) fn receipt_response(
    shared: &Shared,
    mut receipt: storage::MobileCommandReceipt,
) -> Value {
    if receipt.state == "accepted"
        && !shared.command_pending(&receipt.device_id, &receipt.operation_id)
    {
        receipt.state = "outcome_unknown".into();
        receipt.error = Some("outcome_unknown".into());
    }
    json!({"status":"receipt","receipt":receipt})
}

fn validate(request: &MobileCommandRequest) -> Result<(), &'static str> {
    request.command.validate_feedback_target()?;
    if !valid_id(&request.operation_id)
        || !valid_id(&request.workspace_id)
        || !valid_id(&request.route_id)
        || request.grant_revision == 0
    {
        return Err("invalid_request");
    }
    match &request.command {
        MobileCommand::TerminalInput {
            expected, input, ..
        } if !valid_target(expected) || expected.run_id.is_none() || !input.valid() => {
            Err("invalid_request")
        }
        MobileCommand::AcceptDelivery { key, fingerprint }
            if key.validate().is_err()
                || fingerprint.len() != 64
                || !fingerprint.bytes().all(|c| c.is_ascii_hexdigit()) =>
        {
            Err("invalid_request")
        }
        MobileCommand::DecidePlanTodo {
            retry_of: Some(previous),
            choice,
            ..
        } if !valid_id(&previous.device_id)
            || !valid_id(&previous.operation_id)
            || !matches!(choice, agent::PlanTodoChoice::Accept { .. }) =>
        {
            Err("invalid_request")
        }
        MobileCommand::DecidePlanTodo {
            plan_id,
            todo_index,
            ..
        } if !valid_id(plan_id) || *todo_index >= 8 => Err("invalid_request"),
        MobileCommand::DecidePlanTodo {
            choice: agent::PlanTodoChoice::Accept { target, .. },
            ..
        } if !valid_target(target) => Err("invalid_request"),
        MobileCommand::GeneratePlan {
            priority: Some(priority),
            ..
        } if !valid_id(&priority.plan_id) || priority.todo_index >= 8 => {
            Err("invalid_plan_priority")
        }
        MobileCommand::GeneratePlan { direction, .. }
            if !agent::valid_plan_direction(direction) =>
        {
            Err("invalid_plan_input")
        }
        MobileCommand::DecideApproval { expected, .. } if !expected.valid() => {
            Err("invalid_request")
        }
        MobileCommand::CreateCard { title, .. }
            if title.as_ref().is_some_and(|t| {
                t.trim().is_empty() || t.chars().count() > 80 || t.chars().any(char::is_control)
            }) =>
        {
            Err("invalid_request")
        }
        MobileCommand::SendMessage { text, .. } | MobileCommand::QueueMessage { text, .. }
            if text.trim().is_empty()
                || text.len() > 32 * 1024
                || text
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t') =>
        {
            Err("invalid_message")
        }
        MobileCommand::DeleteCard { expected, .. }
        | MobileCommand::FeedbackDelivery { expected, .. }
        | MobileCommand::SendMessage { expected, .. }
        | MobileCommand::QueueMessage { expected, .. }
        | MobileCommand::InterruptTurn { expected }
            if !valid_target(expected) =>
        {
            Err("invalid_request")
        }
        MobileCommand::InterruptTurn { expected }
            if expected.run_id.is_none()
                || expected.session_id.is_none()
                || expected.turn_id.is_none() =>
        {
            Err("invalid_request")
        }
        MobileCommand::QueueMessage { expected, .. }
            if expected.run_id.is_none() || expected.session_id.is_none() =>
        {
            Err("invalid_request")
        }
        _ => Ok(()),
    }
}

pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn valid_target(target: &norma::mobile::CardSnapshot) -> bool {
    !(target.node_id == 0
        || target.title.chars().count() > 256
        || target
            .task_title
            .as_ref()
            .is_some_and(|s| s.chars().count() > 1024)
        || target.run_id.as_ref().is_some_and(|s| !valid_id(s))
        || target.session_id.as_ref().is_some_and(|s| !valid_id(s))
        || target.turn_id.as_ref().is_some_and(|s| !valid_id(s)))
}
