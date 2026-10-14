use crate::app::MultisoulApp;
use gpui::Window;
use norma::protocol::{
    AgentErrorCode, AgentProfile, AgentTaskReceiptStatus, AgentTaskStatus, CardCommand,
    CardCommandError, CardCommandResult,
};

pub(crate) fn execute_orchestration_agent_command(
    app: &mut MultisoulApp,
    window: Option<&mut Window>,
    command: CardCommand,
    source: agent::AgentDispatchSource,
    source_provider: AgentProfile,
) -> Result<CardCommandResult, CardCommandError> {
    execute_orchestration_agent_command_with_visual_origin(
        app,
        window,
        command,
        source,
        source_provider,
        dispatch_visual_origin(source),
    )
}

pub(crate) fn execute_orchestration_agent_command_with_visual_origin(
    app: &mut MultisoulApp,
    window: Option<&mut Window>,
    command: CardCommand,
    source: agent::AgentDispatchSource,
    source_provider: AgentProfile,
    visual_origin: Option<crate::motion::DispatchVisualOrigin>,
) -> Result<CardCommandResult, CardCommandError> {
    match command {
        CardCommand::AgentTaskStatus { task_ref } => task_status(app, &task_ref),
        CardCommand::AgentTaskResult { task_ref } => task_result(app, &task_ref),
        CardCommand::AgentTaskCancel { task_ref } => cancel_task(app, &task_ref),
        command => {
            let area = app.mobile_canvas_area(window);
            let zoom = app.runtime.document.viewport.zoom;
            super::commands::execute_coordinated_agent_command(
                app,
                command,
                area,
                zoom,
                source,
                Some(source_provider),
                super::commands::DispatchOptions {
                    visual_origin,
                    target_node_id: None,
                },
            )
        }
    }
}

pub(crate) fn execute_high_level_agent_command(
    app: &mut MultisoulApp,
    command: CardCommand,
    area: canvas::CanvasRect,
    zoom: f32,
    source: agent::AgentDispatchSource,
    source_provider: Option<AgentProfile>,
) -> Result<CardCommandResult, CardCommandError> {
    match command {
        CardCommand::AgentTaskStatus { task_ref } => task_status(app, &task_ref),
        CardCommand::AgentTaskResult { task_ref } => task_result(app, &task_ref),
        CardCommand::AgentTaskCancel { task_ref } => cancel_task(app, &task_ref),
        command => super::commands::execute_coordinated_agent_command(
            app,
            command,
            area,
            zoom,
            source,
            source_provider,
            super::commands::DispatchOptions {
                visual_origin: dispatch_visual_origin(source),
                target_node_id: None,
            },
        ),
    }
}

fn dispatch_visual_origin(
    source: agent::AgentDispatchSource,
) -> Option<crate::motion::DispatchVisualOrigin> {
    match source.queue_source {
        agent::AgentQueueTaskSource::Norma => {
            Some(crate::motion::DispatchVisualOrigin::CommandDock)
        }
        agent::AgentQueueTaskSource::Agent { .. } => source
            .node_id
            .map(crate::motion::DispatchVisualOrigin::AgentNode),
        agent::AgentQueueTaskSource::Manual => None,
    }
}

fn task_status(app: &MultisoulApp, task_ref: &str) -> Result<CardCommandResult, CardCommandError> {
    let coordinator = app.agent_task_coordinator.as_ref().ok_or_else(|| {
        CardCommandError::agent(
            AgentErrorCode::RuntimeExpired,
            "任务引用属于已结束的 App 运行实例",
            false,
        )
    })?;
    let task = coordinator
        .task_by_ref(task_ref)
        .map_err(task_reference_error)?;
    let pending = app
        .pending_agent_submissions
        .task_stage(task.task_id)
        .is_some()
        && matches!(task.state, "queued" | "running" | "cancel_requested");
    let mut status = wire_task_status(task);
    if pending {
        status.status = AgentTaskReceiptStatus::Queued;
    }
    Ok(CardCommandResult::AgentTaskStatus { task: status })
}

fn task_result(app: &MultisoulApp, task_ref: &str) -> Result<CardCommandResult, CardCommandError> {
    let CardCommandResult::AgentTaskStatus { task } = task_status(app, task_ref)? else {
        unreachable!("task_status always returns AgentTaskStatus");
    };
    if !matches!(
        task.status,
        AgentTaskReceiptStatus::Done
            | AgentTaskReceiptStatus::Failed
            | AgentTaskReceiptStatus::Cancelled
            | AgentTaskReceiptStatus::Interrupted
    ) {
        return Err(CardCommandError::agent_with_status(
            AgentErrorCode::TaskResultNotReady,
            "任务尚未进入终态，结果暂不可用",
            true,
            task.status,
        ));
    }
    Ok(CardCommandResult::AgentTaskResult { task })
}

fn cancel_task(
    app: &mut MultisoulApp,
    task_ref: &str,
) -> Result<CardCommandResult, CardCommandError> {
    let task = app
        .agent_task_coordinator
        .as_ref()
        .ok_or_else(|| {
            CardCommandError::agent(
                AgentErrorCode::RuntimeExpired,
                "任务引用属于已结束的 App 运行实例",
                false,
            )
        })?
        .task_by_ref(task_ref)
        .map_err(task_reference_error)?;
    if let Some((node_id, task_was_typed)) = app.pending_agent_submissions.task_stage(task.task_id)
    {
        if task_was_typed {
            super::command_support::clear_agent_task_input(&mut app.runtime, node_id).map_err(
                |error| {
                    CardCommandError::agent(
                        AgentErrorCode::InternalError,
                        format!("无法清除尚未提交的任务输入：{error}"),
                        true,
                    )
                },
            )?;
        }
        app.pending_agent_submissions
            .remove_task(task.task_id)
            .expect("pending task stage was found immediately above");
        let (cancellation, next) = app
            .agent_task_coordinator
            .as_mut()
            .expect("coordinator was present while resolving task")
            .cancel_unsubmitted_task(task.task_id);
        if let Some(next) = next {
            app.pending_agent_submissions.enqueue_immediate(next);
        }
        if cancellation != agent::AgentTaskCancellation::Cancelled {
            return Err(CardCommandError::agent(
                AgentErrorCode::TaskNotFound,
                "未找到可取消的尚未提交任务",
                false,
            ));
        }
        if let Some(writer) = app.persistence.agent_dispatch_writer() {
            writer.persist_cancellation(task.task_id, false);
        }
        return Ok(CardCommandResult::AgentTaskCancelled {
            task_id: task.task_ref,
            status: AgentTaskReceiptStatus::Cancelled,
        });
    }
    let cancel_kind = task_cancel_kind(task.state);
    match cancel_kind {
        TaskCancelKind::Terminal => {
            return Err(CardCommandError::agent(
                AgentErrorCode::TaskNotFound,
                "任务已经结束，无法再次取消",
                false,
            ));
        }
        TaskCancelKind::CancelRequested => {
            return Ok(CardCommandResult::AgentTaskCancelled {
                task_id: task.task_ref,
                status: AgentTaskReceiptStatus::CancelRequested,
            });
        }
        TaskCancelKind::Running => {
            app.runtime
                .send_node_input(
                    task.target_node_id,
                    runtime::NodeInput::Key(terminal::KeyInput {
                        key: terminal::Key::Char('c'),
                        ctrl: true,
                        alt: false,
                        shift: false,
                    }),
                )
                .map_err(|error| {
                    CardCommandError::agent(
                        AgentErrorCode::InternalError,
                        format!("无法向运行任务发送中断：{error}"),
                        true,
                    )
                })?;
        }
        TaskCancelKind::Queued => {}
    }
    let cancellation = app
        .agent_task_coordinator
        .as_mut()
        .expect("coordinator was present while resolving task")
        .cancel_task_ref(task_ref)
        .map_err(task_reference_error)?;
    let status = match cancellation {
        agent::AgentTaskCancellation::Cancelled => {
            app.pending_agent_submissions.remove_task(task.task_id);
            AgentTaskReceiptStatus::Cancelled
        }
        agent::AgentTaskCancellation::CancelRequested => AgentTaskReceiptStatus::CancelRequested,
        agent::AgentTaskCancellation::AlreadySubmitted => {
            return Err(CardCommandError::agent(
                AgentErrorCode::TaskNotCancellable,
                "消息已交给 Agent CLI，请在卡片内管理其消息队列",
                false,
            ));
        }
        agent::AgentTaskCancellation::NotFound => {
            return Err(CardCommandError::agent(
                AgentErrorCode::TaskNotFound,
                "未找到可取消的任务",
                false,
            ));
        }
    };
    if let Some(writer) = app.persistence.agent_dispatch_writer() {
        writer.persist_cancellation(task.task_id, cancel_kind == TaskCancelKind::Running);
    }
    Ok(CardCommandResult::AgentTaskCancelled {
        task_id: task.task_ref,
        status,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskCancelKind {
    Queued,
    Running,
    CancelRequested,
    Terminal,
}

fn task_cancel_kind(state: &str) -> TaskCancelKind {
    match state {
        "queued" => TaskCancelKind::Queued,
        "running" => TaskCancelKind::Running,
        "cancel_requested" => TaskCancelKind::CancelRequested,
        _ => TaskCancelKind::Terminal,
    }
}

fn wire_task_status(task: agent::AgentCoordinatedTask) -> AgentTaskStatus {
    let tracking = task.tracking;
    let status = if tracking == agent::AgentTrackingMode::Untracked
        && matches!(task.state, "running" | "cancel_requested")
    {
        AgentTaskReceiptStatus::Untracked
    } else {
        wire_status(task.state)
    };
    AgentTaskStatus {
        task_id: task.task_ref,
        target_definition_id: task.definition_id,
        target_name: task.agent,
        target_node_id: task.target_node_id,
        status,
        tracking: match tracking {
            agent::AgentTrackingMode::Managed => "managed",
            agent::AgentTrackingMode::Untracked => "untracked",
        }
        .to_string(),
        parent_task_id: task.parent_task_ref,
        root_task_id: task.root_task_ref,
        result: task.result.or_else(|| {
            matches!(
                status,
                AgentTaskReceiptStatus::Done
                    | AgentTaskReceiptStatus::Failed
                    | AgentTaskReceiptStatus::Cancelled
                    | AgentTaskReceiptStatus::Interrupted
            )
            .then_some(norma::protocol::AgentTaskResult {
                availability: norma::protocol::AgentTaskResultAvailability::Unavailable,
                text: None,
                truncated: false,
            })
        }),
    }
}

fn wire_status(state: &str) -> AgentTaskReceiptStatus {
    match state {
        "queued" => AgentTaskReceiptStatus::Queued,
        "cancel_requested" => AgentTaskReceiptStatus::CancelRequested,
        "succeeded" => AgentTaskReceiptStatus::Done,
        "failed" => AgentTaskReceiptStatus::Failed,
        "cancelled" => AgentTaskReceiptStatus::Cancelled,
        "interrupted" => AgentTaskReceiptStatus::Interrupted,
        _ => AgentTaskReceiptStatus::Submitted,
    }
}

fn task_reference_error(error: agent::AgentTaskReferenceError) -> CardCommandError {
    match error {
        agent::AgentTaskReferenceError::RuntimeExpired => {
            CardCommandError::agent(AgentErrorCode::RuntimeExpired, error.to_string(), false)
        }
        agent::AgentTaskReferenceError::Invalid | agent::AgentTaskReferenceError::NotFound => {
            CardCommandError::agent(AgentErrorCode::TaskNotFound, error.to_string(), false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{task_cancel_kind, wire_task_status, TaskCancelKind};
    use norma::protocol::AgentTaskReceiptStatus;

    /// 已完成任务的取消分类：终态任务不得再触发终端中断
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   terminal_states = 4 个协议终态（succeeded / failed / cancelled / interrupted）
    ///   running_states  = 2 个非终态（queued / running）
    ///   Total           = 4 + 2 = 6 个代表状态
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 逐一分类 4 个终态 → 均得到 Terminal
    ///   2. 分类 queued → 得到 Queued，不发送 Ctrl-C
    ///   3. 分类 running → 得到 Running，仅该状态允许发送 Ctrl-C
    ///
    /// 预期结果：
    ///   - 正断言：running 明确属于 Running
    ///   - 负断言：所有终态均不属于 Running，避免误中断同一卡片上的新任务
    #[test]
    fn completed_task_cancel_never_targets_the_terminal() {
        assert_eq!(
            task_cancel_kind("succeeded"),
            TaskCancelKind::Terminal,
            "succeeded task must be rejected before sending Ctrl-C"
        );
        assert_eq!(
            task_cancel_kind("failed"),
            TaskCancelKind::Terminal,
            "failed task must be rejected before sending Ctrl-C"
        );
        assert_eq!(
            task_cancel_kind("cancelled"),
            TaskCancelKind::Terminal,
            "cancelled task must be rejected before sending Ctrl-C"
        );
        assert_eq!(
            task_cancel_kind("interrupted"),
            TaskCancelKind::Terminal,
            "interrupted task must be rejected before sending Ctrl-C"
        );
        assert_eq!(
            task_cancel_kind("queued"),
            TaskCancelKind::Queued,
            "queued task should be removed without sending Ctrl-C"
        );
        assert_eq!(
            task_cancel_kind("running"),
            TaskCancelKind::Running,
            "only a running task should send Ctrl-C to its target node"
        );
        assert_ne!(
            task_cancel_kind("succeeded"),
            TaskCancelKind::Running,
            "a completed task must not be treated as the currently running task"
        );
    }

    /// Untracked 状态投影：缺少 Provider Hook 的运行任务不得伪装为可等待的 Submitted。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   coordinator state = running × 1
    ///   tracking          = untracked × 1
    ///   pending override  = 0
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 构造协调器仍在运行、但 Provider Hook 不完整的任务。
    ///   2. 经过 wire 状态投影。
    ///   3. 检查协议状态与 tracking 字段。
    ///
    /// 预期结果：
    ///   - 正断言：协议状态为 Untracked，tracking 字段为 untracked。
    ///   - 负断言：不得返回 Submitted 并让 wait 进入轮询。
    #[test]
    fn running_untracked_task_projects_untracked_wire_status() {
        let status = wire_task_status(agent::AgentCoordinatedTask {
            task_id: 1,
            task_ref: "runtime:1".to_string(),
            root_task_ref: "runtime:1".to_string(),
            parent_task_ref: None,
            source_definition_id: None,
            source_node_id: None,
            definition_id: 7,
            target_node_id: 11,
            agent: "Alan".to_string(),
            task: "无 Hook 任务".to_string(),
            state: "running",
            tracking: agent::AgentTrackingMode::Untracked,
            result: None,
        });

        assert_eq!(
            status.status,
            AgentTaskReceiptStatus::Untracked,
            "运行中的 untracked 任务必须投影为 Untracked"
        );
        assert_eq!(
            status.tracking, "untracked",
            "协议 tracking 字段必须明确保留 untracked"
        );
        assert_ne!(
            status.status,
            AgentTaskReceiptStatus::Submitted,
            "untracked 任务不得伪装为可轮询的 Submitted"
        );
    }
}
