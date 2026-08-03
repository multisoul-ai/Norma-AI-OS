use super::{
    agent_error, closed_workspace_response, execute_validated_in_workspace, new_workspace_route_id,
    receipt_state, resolve_target_definition, resolve_targets, response_error_code,
    routing_error_response, serialized_profile, source_active_task_ref, validate_source,
    NormaEndpointHost,
};
use gpui::App;
use norma::protocol::{AgentErrorCode, CardCommandResult, WireRequest, WireResponse};
use sha2::{Digest, Sha256};

pub(super) fn spawn_global_dispatch(
    request: WireRequest,
    response_sender: crossbeam_channel::Sender<WireResponse>,
    cx: &mut App,
) {
    let request_id = request.request_id;
    let routed = match resolve_targets(&request, cx) {
        Ok(routed) => routed,
        Err(error) => {
            let _ = response_sender.send(routing_error_response(request_id, error));
            return;
        }
    };
    let (source, source_provider) = match validate_source(&routed.source, &request, cx) {
        Ok(source) => source,
        Err(response) => {
            let _ = response_sender.send(response);
            return;
        }
    };
    let source_local_parent_ref = source_active_task_ref(&routed.source, source, cx);
    let target_definition_id = match resolve_target_definition(&routed.target, &request, cx) {
        Ok(definition_id) => definition_id,
        Err(response) => {
            let _ = response_sender.send(response);
            return;
        }
    };
    let Some(operation_id) = request.operation_id.clone() else {
        let _ = response_sender.send(agent_error(
            request_id,
            AgentErrorCode::ClientUpgradeRequired,
            "任务派发缺少 operation_id；请升级 Norma CLI",
            false,
        ));
        return;
    };
    let Some(task_store) = cx
        .try_global::<NormaEndpointHost>()
        .and_then(|host| host.task_store.clone())
    else {
        let _ = response_sender.send(agent_error(
            request_id,
            AgentErrorCode::OrchestrationChecking,
            "全局任务账本尚未就绪，请稍后使用同一 operation_id 重试",
            true,
        ));
        return;
    };
    let source_definition_id = source
        .definition_id
        .expect("validated Agent source always has a definition id");
    let source_node_id = source
        .node_id
        .expect("validated Agent source always has a node id");
    let global_task_ref = format!(
        "task-{}",
        new_workspace_route_id().unwrap_or_else(|_| format!("{}-{request_id}", std::process::id()))
    );
    let fingerprint = request_fingerprint(&request, &routed.target.summary.workspace_id);
    let mut reservation = storage::NewOrchestrationTaskRoute {
        task_ref: global_task_ref,
        operation_id: operation_id.clone(),
        request_fingerprint: fingerprint,
        source_workspace_id: routed.source.summary.workspace_id.clone(),
        source_definition_id,
        source_provider: serialized_profile(source_provider),
        source_node_id_at_dispatch: source_node_id,
        target_workspace_id: routed.target.summary.workspace_id.clone(),
        target_definition_id,
        parent_task_ref: None,
        root_task_ref: String::new(),
    };
    reservation.root_task_ref = reservation.task_ref.clone();
    let same_workspace = routed.source.summary.workspace_id == routed.target.summary.workspace_id;
    let target = routed.target;
    let command = request.command;
    let source_workspace_id = reservation.source_workspace_id.clone();
    let target_workspace_id = reservation.target_workspace_id.clone();
    cx.spawn(async move |cx| {
        if let Some(local_parent_ref) = source_local_parent_ref {
            let parent_store = task_store.clone();
            let parent_workspace = source_workspace_id.clone();
            let parent = cx
                .background_executor()
                .spawn(async move {
                    parent_store.find_by_target_task(&parent_workspace, &local_parent_ref)
                })
                .await;
            match parent {
                Ok(Some(parent)) => {
                    reservation.parent_task_ref = Some(parent.task_ref);
                    reservation.root_task_ref = parent.root_task_ref;
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, %operation_id, "failed to resolve global dispatch parent");
                    let _ = response_sender.send(agent_error(
                        request_id,
                        AgentErrorCode::OrchestrationChecking,
                        "全局任务 lineage 暂时不可读；尚未产生派发副作用",
                        true,
                    ));
                    return;
                }
            }
        }
        let reserve_store = task_store.clone();
        let reserved = cx
            .background_executor()
            .spawn(async move { reserve_store.reserve(&reservation) })
            .await;
        let reserved = match reserved {
            Ok(reserved) => reserved,
            Err(error) => {
                tracing::warn!(%error, %operation_id, "failed to reserve global orchestration task");
                let _ = response_sender.send(WireResponse::new(
                    request_id,
                    Err(norma::protocol::CardCommandError::outcome_unknown(
                        &operation_id,
                        "无法确认任务是否被接受",
                    )),
                ));
                return;
            }
        };
        match reserved {
            storage::OrchestrationTaskReservation::Conflict => {
                let _ = response_sender.send(agent_error(
                    request_id,
                    AgentErrorCode::IdempotencyConflict,
                    "同一 operation_id 已用于不同任务参数",
                    false,
                ));
            }
            storage::OrchestrationTaskReservation::Rejected(rejection) => {
                let (code, message) = match rejection {
                    storage::OrchestrationLineageRejection::SelfDispatch => (
                        AgentErrorCode::SelfDispatchForbidden,
                        "Agent 不得向同一 Workspace 中的自己派发任务".to_string(),
                    ),
                    storage::OrchestrationLineageRejection::Cycle => (
                        AgentErrorCode::DelegationCycle,
                        "目标 Agent 已位于当前跨 Workspace 祖先链".to_string(),
                    ),
                    storage::OrchestrationLineageRejection::DescendantBudgetExceeded { limit } => (
                        AgentErrorCode::DelegationBudgetExceeded,
                        format!("根任务已达到 {limit} 个未完成后代"),
                    ),
                };
                let _ = response_sender.send(agent_error(request_id, code, &message, false));
            }
            storage::OrchestrationTaskReservation::Replay(existing) => {
                let replay = existing
                    .result_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok());
                let response = replay.map_or_else(
                    || {
                        agent_error(
                            request_id,
                            AgentErrorCode::OrchestrationChecking,
                            "相同 operation_id 的首次请求仍在处理",
                            true,
                        )
                    },
                    |result| WireResponse::new(request_id, result),
                );
                let _ = response_sender.send(response);
            }
            storage::OrchestrationTaskReservation::New(stored) => {
                let visual_origin = same_workspace
                    .then_some(crate::motion::DispatchVisualOrigin::AgentNode(source_node_id));
                let target_source = if same_workspace {
                    source
                } else {
                    agent::AgentDispatchSource::norma()
                };
                let executed = cx.update(|cx| {
                    execute_validated_in_workspace(
                        target,
                        request_id,
                        command,
                        target_source,
                        source_provider,
                        visual_origin,
                        cx,
                    )
                });
                let mut response = executed.unwrap_or_else(|_| closed_workspace_response(request_id));
                let local_task = globalize_dispatch_response(
                    &mut response,
                    &stored.task_ref,
                    &operation_id,
                    &source_workspace_id,
                    &target_workspace_id,
                    stored.parent_task_ref.as_deref(),
                    &stored.root_task_ref,
                );
                let result_json = serde_json::to_string(&response.result);
                let persist_result = match (local_task, result_json) {
                    (Some((local_ref, definition_id, name, node_id, state, tracking)), Ok(json)) => {
                        let complete_store = task_store.clone();
                        let complete_operation = operation_id.clone();
                        cx.background_executor()
                            .spawn(async move {
                                complete_store.complete(
                                    &complete_operation,
                                    &local_ref,
                                    definition_id,
                                    &name,
                                    node_id,
                                    state,
                                    &tracking,
                                    &json,
                                )
                            })
                            .await
                    }
                    (None, Ok(json)) => {
                        let complete_store = task_store.clone();
                        let complete_operation = operation_id.clone();
                        let error_code = response_error_code(&response).unwrap_or("internal_error");
                        cx.background_executor()
                            .spawn(async move {
                                complete_store.complete_error(
                                    &complete_operation,
                                    error_code,
                                    &json,
                                )
                            })
                            .await
                    }
                    (_, Err(error)) => Err(error.into()),
                };
                if let Err(error) = persist_result {
                    tracing::warn!(%error, %operation_id, "task executed but global route persistence failed");
                    response = WireResponse::new(
                        request_id,
                        Err(norma::protocol::CardCommandError::outcome_unknown(
                            &operation_id,
                            "任务已执行但结果持久化失败",
                        )),
                    );
                }
                let _ = response_sender.send(response);
            }
        }
    })
    .detach();
}

fn globalize_dispatch_response(
    response: &mut WireResponse,
    global_task_ref: &str,
    operation_id: &str,
    source_workspace_id: &str,
    target_workspace_id: &str,
    parent_task_ref: Option<&str>,
    root_task_ref: &str,
) -> Option<(String, i64, String, u64, &'static str, String)> {
    let Ok(CardCommandResult::AgentTasksDispatched { receipts }) = &mut response.result else {
        return None;
    };
    let receipt = receipts.first_mut()?;
    let local_task_ref = std::mem::replace(&mut receipt.task_id, global_task_ref.to_owned());
    receipt.operation_id = Some(operation_id.to_owned());
    receipt.source_workspace_id = Some(source_workspace_id.to_owned());
    receipt.target_workspace_id = Some(target_workspace_id.to_owned());
    receipt.parent_task_id = parent_task_ref.map(str::to_owned);
    receipt.root_task_id = root_task_ref.to_owned();
    Some((
        local_task_ref,
        receipt.target_definition_id,
        receipt.name.clone(),
        receipt.node_id,
        receipt_state(receipt.status),
        receipt.tracking.clone(),
    ))
}

fn request_fingerprint(request: &WireRequest, target_workspace_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(request.source_workspace_id.as_deref().unwrap_or_default());
    hasher.update([0]);
    hasher.update(target_workspace_id);
    hasher.update([0]);
    hasher.update(serde_json::to_vec(&request.command).unwrap_or_default());
    hex::encode(hasher.finalize())
}
