//! Foreground boundary: exact retained area, four AdapterCommands, no window or database reads.
use agent::{AdapterCommand, AdapterError, AdapterResult, AgentInputSource};
use gpui::App;
use norma::connector::{CardSigner, Snapshot, WriteRequest};
use std::{path::Path, time::Instant};
use storage::DesktopProjectArea;

pub(super) struct ExecutionCall {
    pub request: WriteRequest,
    pub workspace: String,
    pub area: String,
    pub deadline: Instant,
    pub cancellation: tokio_util::sync::CancellationToken,
    pub reply: crossbeam_channel::Sender<Result<Executed, AdapterError>>,
}
pub(super) struct Executed {
    pub receipt: AdapterResult,
    pub task_ref: Option<String>,
    pub task_state: Option<String>,
    pub tracking: Option<String>,
}
pub(super) fn execute(
    database: &Path,
    call: ExecutionCall,
    areas: &[DesktopProjectArea],
    snapshot: &Snapshot,
    signer: &CardSigner,
    cx: &mut App,
) {
    let result = (|| {
        if call.cancellation.is_cancelled() || Instant::now() >= call.deadline {
            return Err(reject("outcome_unknown"));
        }
        let area = areas
            .iter()
            .find(|a| a.desktop_workspace_id == call.workspace && a.id == call.area)
            .ok_or_else(|| reject("area_not_found"))?;
        let session = crate::workspace::desktop::mobile::session(database, area, cx)
            .ok_or_else(|| reject("workspace_closed"))?;
        session.update(cx, |app, cx| {
            let command = match &call.request {
                WriteRequest::Dispatch(a) => {
                    let identity = signer.verify(&a.card_id).map_err(reject)?;
                    exact_card(snapshot, &identity, false)?;
                    AdapterCommand::AssignAgentTask {
                        agent: String::new(),
                        task: call.request.task_text().expect("dispatch task"),
                        node_id: Some(identity.node_id),
                        expected_revision: Some(a.card_revision.clone()),
                        target_identity: Some(identity),
                        allow_queue: a.allow_queue,
                    }
                }
                WriteRequest::Reply(a) => {
                    let identity = signer.verify(&a.card_id).map_err(reject)?;
                    exact_card(snapshot, &identity, true)?;
                    AdapterCommand::ReplyAgent {
                        agent: String::new(),
                        message: a.message.clone(),
                        node_id: Some(identity.node_id),
                        expected_revision: Some(a.card_revision.clone()),
                        target_identity: Some(identity),
                    }
                }
                WriteRequest::Cancel(a) => {
                    let task_id = app
                        .agent_task_coordinator
                        .as_ref()
                        .and_then(|c| c.task_by_ref(&a.task_id).ok())
                        .map(|t| t.task_id)
                        .or_else(|| {
                            a.task_id
                                .rsplit_once(':')
                                .and_then(|(_, id)| id.parse::<u64>().ok())
                        })
                        .unwrap_or(0);
                    AdapterCommand::CancelAgentTask {
                        task_id,
                        expected_task_ref: Some(a.task_id.clone()),
                    }
                }
                WriteRequest::Create(a) => AdapterCommand::CreateAgent {
                    profile: match a.profile.as_str() {
                        "claude" => norma::protocol::AgentProfile::Claude,
                        "codex" => norma::protocol::AgentProfile::Codex,
                        _ => return Err(reject("profile_unavailable")),
                    },
                    task: a.task.clone(),
                },
            };
            let receipt = crate::app::execute_norma_adapter_command_with_source(
                app,
                None,
                cx,
                command,
                AgentInputSource::Connector,
            )?;
            let task = receipt.task_id.and_then(|id| {
                app.agent_task_coordinator
                    .as_ref()
                    .and_then(|c| c.tasks().into_iter().find(|t| t.task_id == id))
            });
            Ok(Executed {
                receipt,
                task_ref: task.as_ref().map(|t| t.task_ref.clone()),
                task_state: task.as_ref().map(|t| {
                    if app
                        .pending_agent_submissions
                        .task_stage(t.task_id)
                        .is_some()
                    {
                        "queued".into()
                    } else {
                        t.state.into()
                    }
                }),
                tracking: task.as_ref().map(|t| {
                    if t.tracking == agent::AgentTrackingMode::Managed {
                        "tracked".into()
                    } else {
                        "untracked".into()
                    }
                }),
            })
        })
    })();
    let _ = call.reply.try_send(result);
}
fn exact_card(
    snapshot: &Snapshot,
    identity: &norma::connector::CardIdentity,
    reply: bool,
) -> Result<(), AdapterError> {
    let card = snapshot
        .workspaces
        .iter()
        .filter(|w| w.workspace_id == identity.workspace_id)
        .flat_map(|w| &w.cards)
        .find(|c| c.identity.area_id == identity.area_id && c.identity.node_id == identity.node_id);
    let card = card.ok_or_else(|| {
        reject(if reply {
            "card_not_loaded"
        } else {
            "card_not_found"
        })
    })?;
    if &card.identity != identity {
        return Err(reject("target_changed"));
    }
    if !card.loaded {
        return Err(reject(if reply {
            "card_not_loaded"
        } else {
            "card_not_found"
        }));
    }
    Ok(())
}
fn reject(code: &str) -> AdapterError {
    AdapterError::Rejected(code.into())
}

pub(super) fn error_code(error: &AdapterError, create: bool) -> &'static str {
    match error {
        AdapterError::AlreadySubmitted => "already_submitted",
        AdapterError::Rejected(code)
            if matches!(
                code.as_str(),
                "card_not_found"
                    | "target_changed"
                    | "approval_pending"
                    | "input_occupied"
                    | "untracked_requires_confirmation"
                    | "card_busy"
                    | "card_not_loaded"
                    | "workspace_closed"
                    | "outcome_unknown"
                    | "profile_unavailable"
                    | "area_not_found"
                    | "task_not_found"
            ) =>
        {
            match code.as_str() {
                "task_not_found" => "task_not_found",
                "card_not_found" => "card_not_found",
                "target_changed" => "target_changed",
                "approval_pending" => "approval_pending",
                "input_occupied" => "input_occupied",
                "untracked_requires_confirmation" => "untracked_requires_confirmation",
                "card_busy" => "card_busy",
                "card_not_loaded" => "card_not_loaded",
                "workspace_closed" => "workspace_closed",
                "outcome_unknown" => "outcome_unknown",
                "profile_unavailable" => "profile_unavailable",
                _ => "area_not_found",
            }
        }
        AdapterError::InvalidArguments(_) => "invalid_argument",
        AdapterError::Failed(_) if create => "card_create_failed",
        AdapterError::Unknown(_) | AdapterError::Cancelled => "outcome_unknown",
        _ => "agent_rejected",
    }
}
