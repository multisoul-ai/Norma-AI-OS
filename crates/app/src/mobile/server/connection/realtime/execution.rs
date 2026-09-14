use super::{Session, Shared};
use agent::{AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, ToolContext};
use futures::future::BoxFuture;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};

pub(super) struct Tools {
    pub shared: Arc<Shared>,
    pub session: Arc<Session>,
}

pub(super) fn operation_id(session: &Session, function_call: &str) -> String {
    // Length-delimited JSON avoids collisions at concatenation boundaries. A reconnect's
    // generation/route is deliberately absent: it must query the same durable operation.
    let identity = serde_json::to_vec(&[
        &session.machine,
        &session.device,
        &session.binding.call_id,
        function_call,
    ])
    .expect("string array is serializable");
    format!("rt-{}", hex::encode(Sha256::digest(identity)))
}

impl AgentCardAdapter for Tools {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            if !agent::realtime_command_authorized(&command, &context.user_input) {
                return Err(AdapterError::Rejected(
                    "当前原话没有明确执行这项操作的意图；仅保留讨论或草稿".into(),
                ));
            }
            if self.session.cancelled.is_cancelled() || context.cancellation.is_cancelled() {
                return Err(AdapterError::Cancelled);
            }
            let operation = context
                .operation_id
                .clone()
                .ok_or_else(|| rejected("invalid_request"))?;
            let shared = Arc::clone(&self.shared);
            let session = Arc::clone(&self.session);
            let operation_copy = operation.clone();
            let cancelled = context.cancellation.clone();
            let mut receipt = match tokio::task::spawn_blocking(move || {
                start(&shared, &session, &operation_copy, command, &cancelled)
            })
            .await
            {
                Ok(Ok(receipt)) => receipt,
                // submit may already have dispatched before its final read or worker failed.
                Ok(Err("unavailable")) | Err(_) => return Ok(unverified()),
                Ok(Err(code)) => return Err(rejected(code)),
            };
            // Dispatch has a 30s lease; allow 5s for its final persistence/readback result.
            let deadline = tokio::time::Instant::now() + Duration::from_secs(35);
            loop {
                let revision = crate::mobile::project_events::revision();
                match receipt["receipt"]["state"].as_str() {
                    Some("succeeded") => {
                        return Ok(serde_json::from_value(receipt["receipt"]["result"].clone())
                            .unwrap_or_else(|_| unverified()))
                    }
                    Some("outcome_unknown") => {
                        return Ok(AdapterResult::unknown(
                            "执行结果未知，请核对原回执；不能重复派工",
                        ))
                    }
                    Some("rejected") => {
                        return Err(rejected(
                            receipt["receipt"]["error"].as_str().unwrap_or("rejected"),
                        ))
                    }
                    Some("accepted") => {}
                    _ => return Ok(unverified()),
                }
                tokio::select! {
                    _ = context.cancellation.cancelled() => return Ok(AdapterResult::unknown("通话已插话；已提交任务的结果需查原回执")),
                    _ = self.session.cancelled.cancelled() => return Ok(AdapterResult::unknown("绑定已断开；已提交任务的结果需查原回执")),
                    _ = tokio::time::sleep_until(deadline) => return Ok(AdapterResult::unknown("执行回执超时，不能重复派工")),
                    _ = crate::mobile::project_events::wait(revision, Duration::from_secs(2)) => {}
                }
                let shared = Arc::clone(&self.shared);
                let session = Arc::clone(&self.session);
                let operation = operation.clone();
                receipt = match tokio::task::spawn_blocking(move || {
                    crate::mobile::command::receipt(
                        &shared,
                        &session.machine,
                        &session.device,
                        &session.token,
                        &session.binding.workspace_id,
                        &operation,
                    )
                })
                .await
                {
                    Ok(Ok(receipt)) => receipt,
                    _ => return Ok(unverified()),
                };
            }
        })
    }
}

fn start(
    shared: &Arc<Shared>,
    session: &Session,
    operation: &str,
    command: AdapterCommand,
    cancelled: &tokio_util::sync::CancellationToken,
) -> Result<Value, &'static str> {
    let snapshot = crate::mobile::snapshot::read(
        shared,
        &session.machine,
        &session.device,
        &session.token,
        Some(&session.binding.workspace_id),
    )?;
    if snapshot["grant"]["revision"].as_u64() != Some(session.grant_revision)
        || session.cancelled.is_cancelled()
    {
        return Err("stale_binding");
    }
    let existing = crate::mobile::command::receipt(
        shared,
        &session.machine,
        &session.device,
        &session.token,
        &session.binding.workspace_id,
        operation,
    );
    let route = match &existing {
        // Retain the original route in the request hash when querying across a reopened window.
        // reserve still compares the complete command, so changed arguments cannot replay a receipt.
        Ok(receipt) => receipt["receipt"]["route_id"].as_str(),
        Err("not_found") => snapshot["projects"][0]["route_id"].as_str(),
        Err(error) => return Err(error),
    }
    .ok_or("route_expired")?;
    // Snapshot/receipt IO can outlive the user turn. Crossing this boundary submits the
    // command; interruption after it only stops speech, never cancels an accepted task.
    if cancelled.is_cancelled() || session.cancelled.is_cancelled() {
        return Err("stale_turn");
    }
    crate::mobile::command::submit(
        shared,
        &session.machine,
        &session.device,
        &session.token,
        crate::mobile::MobileCommandRequest {
            operation_id: operation.into(),
            workspace_id: session.binding.workspace_id.clone(),
            route_id: route.into(),
            grant_revision: session.grant_revision,
            command: crate::mobile::MobileCommand::RealtimeTool { command },
        },
    )
}

fn unverified() -> AdapterResult {
    AdapterResult::unknown("无法核对执行回执；可能已经提交，请查询原操作，不能重复派工")
}

fn rejected(code: &str) -> AdapterError {
    AdapterError::Rejected(code.into())
}
