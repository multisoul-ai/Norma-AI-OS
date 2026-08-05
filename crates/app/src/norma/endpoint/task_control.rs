use super::{
    agent_error, closed_workspace_response, execute_validated_in_workspace, receipt_state,
    resolve_targets, routing_error_response, serialized_profile, validate_source,
    NormaEndpointHost,
};
use crate::norma::route::WorkspaceRouteError;
use gpui::App;
use norma::protocol::{
    AgentErrorCode, AgentTaskReceiptStatus, CardCommand, CardCommandResult, WireRequest,
    WireResponse,
};
use sha2::{Digest, Sha256};

pub(super) fn spawn_global_task_control(
    request: WireRequest,
    response_sender: crossbeam_channel::Sender<WireResponse>,
    cx: &mut App,
) {
    let request_id = request.request_id;
    let task_ref = match &request.command {
        CardCommand::AgentTaskStatus { task_ref }
        | CardCommand::AgentTaskResult { task_ref }
        | CardCommand::AgentTaskCancel { task_ref } => task_ref.clone(),
        _ => unreachable!("global task control only accepts status, result, or cancel"),
    };
    let cancel_operation_id = request.operation_id.clone();
    let source_route = match resolve_targets(&request, cx) {
        Ok(routed) => routed.source,
        Err(error) => {
            let _ = response_sender.send(routing_error_response(request_id, error));
            return;
        }
    };
    let validated = match validate_source(&source_route, &request, cx) {
        Ok(validated) => validated,
        Err(response) => {
            let _ = response_sender.send(*response);
            return;
        }
    };
    let source = validated.source;
    let source_provider = validated.provider;
    if matches!(request.command, CardCommand::AgentTaskCancel { .. })
        && request.operation_id.is_none()
    {
        let _ = response_sender.send(agent_error(
            request_id,
            AgentErrorCode::ClientUpgradeRequired,
            "任务取消缺少 operation_id；请升级 Norma CLI",
            false,
        ));
        return;
    }
    let Some(task_store) = cx
        .try_global::<NormaEndpointHost>()
        .and_then(|host| host.task_store.clone())
    else {
        let _ = response_sender.send(agent_error(
            request_id,
            AgentErrorCode::OrchestrationChecking,
            "全局任务账本尚未就绪",
            true,
        ));
        return;
    };
    let source_workspace_id = source_route.summary.workspace_id;
    let source_definition_id = source
        .definition_id
        .expect("validated Agent source always has a definition id");
    let source_provider_slug = serialized_profile(source_provider);
    let command = request.command;
    let is_read_only = matches!(
        command,
        CardCommand::AgentTaskStatus { .. } | CardCommand::AgentTaskResult { .. }
    );
    let is_result = matches!(command, CardCommand::AgentTaskResult { .. });
    cx.spawn(async move |cx| {
        let lookup_store = task_store.clone();
        let lookup_ref = task_ref.clone();
        let route = cx
            .background_executor()
            .spawn(async move { lookup_store.get(&lookup_ref) })
            .await;
        let route = match route {
            Ok(Some(route)) => route,
            Ok(None) => {
                let _ = response_sender.send(agent_error(
                    request_id,
                    AgentErrorCode::TaskNotFound,
                    "未找到全局任务引用",
                    false,
                ));
                return;
            }
            Err(error) => {
                tracing::warn!(%error, %task_ref, "failed to read global task route");
                let _ = response_sender.send(agent_error(
                    request_id,
                    AgentErrorCode::OrchestrationChecking,
                    "全局任务账本暂时不可读",
                    true,
                ));
                return;
            }
        };
        if route.source_workspace_id != source_workspace_id
            || route.source_definition_id != source_definition_id
            || route.source_provider != source_provider_slug
        {
            let _ = response_sender.send(agent_error(
                request_id,
                AgentErrorCode::Unauthorized,
                "当前 AgentPrincipal 无权读取该任务",
                false,
            ));
            return;
        }
        let persisted_cancel_operation = if is_read_only {
            None
        } else {
            let operation_id = cancel_operation_id
                .clone()
                .expect("cancel operation ID validated before background task");
            let fingerprint = cancel_fingerprint(
                &task_ref,
                &source_workspace_id,
                source_definition_id,
                &source_provider_slug,
            );
            let operation_store = task_store.clone();
            let reserve_operation_id = operation_id.clone();
            let reservation = cx
                .background_executor()
                .spawn(async move {
                    operation_store.reserve_operation(&reserve_operation_id, &fingerprint)
                })
                .await;
            match reservation {
                Ok(storage::OrchestrationTaskOperationReservation::New) => Some(operation_id),
                Ok(storage::OrchestrationTaskOperationReservation::Replay(Some(result_json))) => {
                    let result = serde_json::from_str(&result_json).unwrap_or_else(|error| {
                        Err(norma::protocol::CardCommandError::agent(
                            AgentErrorCode::InternalError,
                            format!("无法读取首次取消结果：{error}"),
                            true,
                        ))
                    });
                    let _ = response_sender.send(WireResponse::new(request_id, result));
                    return;
                }
                Ok(storage::OrchestrationTaskOperationReservation::Replay(None)) => {
                    let _ = response_sender.send(agent_error(
                        request_id,
                        AgentErrorCode::OrchestrationChecking,
                        "相同 operation_id 的首次取消仍在处理",
                        true,
                    ));
                    return;
                }
                Ok(storage::OrchestrationTaskOperationReservation::Conflict) => {
                    let _ = response_sender.send(agent_error(
                        request_id,
                        AgentErrorCode::IdempotencyConflict,
                        "同一 operation_id 已用于不同取消参数",
                        false,
                    ));
                    return;
                }
                Err(error) => {
                    tracing::warn!(%error, %operation_id, "failed to reserve cancel operation");
                    let _ = response_sender.send(agent_error(
                        request_id,
                        AgentErrorCode::OrchestrationChecking,
                        "取消幂等账本暂时不可写；尚未发送中断",
                        true,
                    ));
                    return;
                }
            }
        };
        let Some(local_task_ref) = route.target_task_ref.clone() else {
            let _ = response_sender.send(agent_error(
                request_id,
                AgentErrorCode::OrchestrationChecking,
                "任务已接受但目标运行时路由仍在建立",
                true,
            ));
            return;
        };
        let target = cx
            .update(|cx| {
                cx.try_global::<NormaEndpointHost>()
                    .ok_or(WorkspaceRouteError::WorkspaceNotOpen)?
                    .routes
                    .resolve_active(&route.target_workspace_id)
                    .cloned()
            })
            .ok()
            .and_then(Result::ok);
        let mut response = match target {
            Some(target) => {
                let local_command = match command {
                    CardCommand::AgentTaskStatus { .. } => CardCommand::AgentTaskStatus {
                        task_ref: local_task_ref,
                    },
                    CardCommand::AgentTaskResult { .. } => CardCommand::AgentTaskResult {
                        task_ref: local_task_ref,
                    },
                    CardCommand::AgentTaskCancel { .. } => CardCommand::AgentTaskCancel {
                        task_ref: local_task_ref,
                    },
                    _ => unreachable!("task control command changed after classification"),
                };
                cx.update(|cx| {
                    execute_validated_in_workspace(
                        target,
                        request_id,
                        local_command,
                        agent::AgentDispatchSource::norma(),
                        source_provider,
                        None,
                        cx,
                    )
                })
                .unwrap_or_else(|_| closed_workspace_response(request_id))
            }
            None if is_read_only => {
                stored_task_status_response(request_id, &route, true, is_result)
            }
            None => agent_error(
                request_id,
                AgentErrorCode::TaskNotCancellable,
                "目标 Workspace 已关闭，任务已对账为 interrupted",
                false,
            ),
        };
        let status_update = globalize_task_control_response(
            &mut response,
            &task_ref,
            route.parent_task_ref.as_deref(),
            &route.root_task_ref,
        );
        if let Some((state, tracking, task_result_json)) = status_update {
            let update_store = task_store.clone();
            let update_ref = task_ref.clone();
            if let Err(error) = cx
                .background_executor()
                .spawn(async move {
                    update_store.update_status_with_result(
                        &update_ref,
                        state,
                        &tracking,
                        task_result_json.as_deref(),
                    )
                })
                .await
            {
                tracing::warn!(%error, %task_ref, "failed to persist global task status summary");
            }
        }
        if let Some(operation_id) = persisted_cancel_operation {
            let result_json = serde_json::to_string(&response.result);
            let persist_result = match result_json {
                Ok(result_json) => {
                    let operation_store = task_store.clone();
                    let complete_operation_id = operation_id.clone();
                    cx.background_executor()
                        .spawn(async move {
                            operation_store
                                .complete_operation(&complete_operation_id, &result_json)
                        })
                        .await
                }
                Err(error) => Err(error.into()),
            };
            if let Err(error) = persist_result {
                tracing::warn!(%error, %operation_id, "cancel executed but idempotent result persistence failed");
                response = WireResponse::new(
                    request_id,
                    Err(norma::protocol::CardCommandError::outcome_unknown(
                        operation_id,
                        "取消已发送但结果持久化失败",
                    )),
                );
            }
        }
        let _ = response_sender.send(response);
    })
    .detach();
}

fn stored_task_status_response(
    request_id: u64,
    route: &storage::OrchestrationTaskRoute,
    target_closed: bool,
    result_query: bool,
) -> WireResponse {
    let Some(definition_id) = route.target_definition_id else {
        return agent_error(
            request_id,
            AgentErrorCode::OrchestrationChecking,
            "任务目标元数据尚未就绪",
            true,
        );
    };
    let status = if target_closed && !is_terminal_task_state(&route.state) {
        AgentTaskReceiptStatus::Interrupted
    } else {
        stored_receipt_status(&route.state)
    };
    let result = if is_terminal_receipt_status(status) {
        match route.task_result_json.as_deref() {
            Some(json) => match serde_json::from_str(json) {
                Ok(result) => Some(result),
                Err(error) => {
                    tracing::warn!(%error, task_ref = %route.task_ref, "failed to decode persisted Agent task result");
                    return agent_error(
                        request_id,
                        AgentErrorCode::InternalError,
                        "任务结果账本损坏，暂时无法读取",
                        true,
                    );
                }
            },
            None => Some(unavailable_task_result()),
        }
    } else {
        None
    };
    if result_query && result.is_none() {
        return WireResponse::new(
            request_id,
            Err(norma::protocol::CardCommandError::agent_with_status(
                AgentErrorCode::TaskResultNotReady,
                "任务尚未进入终态，结果暂不可用",
                true,
                status,
            )),
        );
    }
    let task = norma::protocol::AgentTaskStatus {
        task_id: route.task_ref.clone(),
        target_definition_id: definition_id,
        target_name: route
            .target_name
            .clone()
            .unwrap_or_else(|| "Agent".to_string()),
        target_node_id: route.target_node_id_at_dispatch.unwrap_or(0),
        status,
        tracking: route.tracking.clone(),
        parent_task_id: route.parent_task_ref.clone(),
        root_task_id: route.root_task_ref.clone(),
        result,
    };
    let result = if result_query {
        CardCommandResult::AgentTaskResult { task }
    } else {
        CardCommandResult::AgentTaskStatus { task }
    };
    WireResponse::new(request_id, Ok(result))
}

fn globalize_task_control_response(
    response: &mut WireResponse,
    global_task_ref: &str,
    parent_task_ref: Option<&str>,
    root_task_ref: &str,
) -> Option<(&'static str, String, Option<String>)> {
    match &mut response.result {
        Ok(CardCommandResult::AgentTaskStatus { task })
        | Ok(CardCommandResult::AgentTaskResult { task }) => {
            task.task_id = global_task_ref.to_owned();
            task.parent_task_id = parent_task_ref.map(str::to_owned);
            task.root_task_id = root_task_ref.to_owned();
            let task_result_json = task
                .result
                .as_ref()
                .and_then(|result| serde_json::to_string(result).ok());
            Some((
                receipt_state(task.status),
                task.tracking.clone(),
                task_result_json,
            ))
        }
        Ok(CardCommandResult::AgentTaskCancelled { task_id, status }) => {
            *task_id = global_task_ref.to_owned();
            Some((receipt_state(*status), "managed".to_string(), None))
        }
        _ => None,
    }
}

fn stored_receipt_status(state: &str) -> AgentTaskReceiptStatus {
    match state {
        "done" => AgentTaskReceiptStatus::Done,
        "failed" => AgentTaskReceiptStatus::Failed,
        "cancelled" => AgentTaskReceiptStatus::Cancelled,
        "interrupted" => AgentTaskReceiptStatus::Interrupted,
        "queued" => AgentTaskReceiptStatus::Queued,
        "cancel_requested" => AgentTaskReceiptStatus::CancelRequested,
        "accepted" => AgentTaskReceiptStatus::Accepted,
        _ => AgentTaskReceiptStatus::Submitted,
    }
}

fn is_terminal_task_state(state: &str) -> bool {
    matches!(state, "done" | "failed" | "cancelled" | "interrupted")
}

fn is_terminal_receipt_status(status: AgentTaskReceiptStatus) -> bool {
    matches!(
        status,
        AgentTaskReceiptStatus::Done
            | AgentTaskReceiptStatus::Failed
            | AgentTaskReceiptStatus::Cancelled
            | AgentTaskReceiptStatus::Interrupted
    )
}

fn unavailable_task_result() -> norma::protocol::AgentTaskResult {
    norma::protocol::AgentTaskResult {
        availability: norma::protocol::AgentTaskResultAvailability::Unavailable,
        text: None,
        truncated: false,
    }
}

fn cancel_fingerprint(
    task_ref: &str,
    source_workspace_id: &str,
    source_definition_id: i64,
    source_provider: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"cancel\0");
    hasher.update(task_ref.as_bytes());
    hasher.update([0]);
    hasher.update(source_workspace_id.as_bytes());
    hasher.update([0]);
    hasher.update(source_definition_id.to_le_bytes());
    hasher.update([0]);
    hasher.update(source_provider.as_bytes());
    hex::encode(hasher.finalize())
}
