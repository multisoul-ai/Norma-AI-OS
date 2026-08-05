use crate::cli::{
    render_agent_self_success, render_protocol_error, render_protocol_success, CliInvocation,
    CliOutput, OutputFormat,
};
use crate::client;
use crate::protocol::{
    AgentErrorCode, AgentProfile, AgentSummary, AgentTaskReceiptStatus, CardCommand,
    CardCommandError, CardCommandResult,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitStatusDecision {
    Complete,
    Continue,
    Untracked,
}

pub fn classify_wait_status(status: AgentTaskReceiptStatus) -> WaitStatusDecision {
    match status {
        AgentTaskReceiptStatus::Done
        | AgentTaskReceiptStatus::Failed
        | AgentTaskReceiptStatus::Cancelled
        | AgentTaskReceiptStatus::Interrupted => WaitStatusDecision::Complete,
        AgentTaskReceiptStatus::Untracked => WaitStatusDecision::Untracked,
        AgentTaskReceiptStatus::Accepted
        | AgentTaskReceiptStatus::Submitted
        | AgentTaskReceiptStatus::Queued
        | AgentTaskReceiptStatus::CancelRequested => WaitStatusDecision::Continue,
    }
}

pub fn classify_wait_task(status: AgentTaskReceiptStatus, tracking: &str) -> WaitStatusDecision {
    let status_decision = classify_wait_status(status);
    if status_decision == WaitStatusDecision::Complete {
        return status_decision;
    }
    if tracking != "untracked" {
        status_decision
    } else {
        WaitStatusDecision::Untracked
    }
}

pub fn run_invocation(invocation: CliInvocation) -> CliOutput {
    let invocation = match prepare_invocation(invocation) {
        Ok(invocation) => invocation,
        Err((format, error)) => {
            return render_error(format, local_request_id(), &error);
        }
    };
    match invocation.wait_timeout_seconds {
        Some(timeout_seconds) => run_wait(invocation, Duration::from_secs(timeout_seconds)),
        None => run_once(invocation),
    }
}

fn prepare_invocation(
    mut invocation: CliInvocation,
) -> Result<CliInvocation, (OutputFormat, CardCommandError)> {
    let CardCommand::Present { target, create_new } = invocation.command else {
        return Ok(invocation);
    };
    let format = invocation.format;
    let target = crate::presentation::prepare_presentation_target(target)
        .map_err(|error| (format, error.into_card_error()))?;
    invocation.command = CardCommand::Present { target, create_new };
    Ok(invocation)
}

fn run_once(invocation: CliInvocation) -> CliOutput {
    match client::send_command_with_request_metadata(
        invocation.command,
        invocation.target_workspace_id,
        invocation.operation_id,
    ) {
        Ok(response) => {
            if invocation.project_self {
                render_self(
                    invocation.format,
                    response.request_id.to_string(),
                    &response.result,
                )
            } else {
                render_success(
                    invocation.format,
                    response.request_id.to_string(),
                    &response.result,
                )
            }
        }
        Err(error) => render_error(
            invocation.format,
            local_request_id(),
            &error.into_card_error(),
        ),
    }
}

fn run_wait(invocation: CliInvocation, timeout: Duration) -> CliOutput {
    let started = Instant::now();
    loop {
        let response = match client::send_command_with_request_metadata(
            invocation.command.clone(),
            invocation.target_workspace_id.clone(),
            invocation.operation_id.clone(),
        ) {
            Ok(response) => response,
            Err(error) => {
                return render_error(
                    invocation.format,
                    local_request_id(),
                    &error.into_card_error(),
                );
            }
        };
        let CardCommandResult::AgentTaskStatus { task } = &response.result else {
            return render_error(
                invocation.format,
                response.request_id.to_string(),
                &CardCommandError::agent(
                    AgentErrorCode::InternalError,
                    "task wait 收到了非任务状态响应",
                    false,
                ),
            );
        };
        match classify_wait_task(task.status, &task.tracking) {
            WaitStatusDecision::Complete => {
                return render_success(
                    invocation.format,
                    response.request_id.to_string(),
                    &response.result,
                );
            }
            WaitStatusDecision::Untracked => {
                return render_error(
                    invocation.format,
                    response.request_id.to_string(),
                    &CardCommandError::agent_with_status(
                        AgentErrorCode::TaskUntracked,
                        "任务缺少可信 Hook，无法可靠等待终态",
                        false,
                        task.status,
                    ),
                );
            }
            WaitStatusDecision::Continue => {}
        }
        if started.elapsed() >= timeout {
            return render_error(
                invocation.format,
                response.request_id.to_string(),
                &CardCommandError::agent_with_status(
                    AgentErrorCode::WaitTimeout,
                    "等待任务终态超时；任务未被取消",
                    true,
                    task.status,
                ),
            );
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        std::thread::sleep(WAIT_POLL_INTERVAL.min(remaining));
    }
}

/// `agent self` 投影结果：当前 Agent Session 的稳定身份。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentSelfInfo {
    pub workspace_id: String,
    pub definition_id: i64,
    pub node_id: u64,
    pub name: String,
    pub speech_alias: Option<String>,
    pub profile: AgentProfile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectSelfError {
    NotFound,
    Ambiguous,
    MissingContext,
}

/// 从 Agent roster 中精确投影当前 Session 的自我身份（纯函数，可独立测试）。
///
/// 只用 `active_node_id` 精确匹配进程 node_id，绝不按名称猜测。匹配数不是恰好 1 时
/// 返回确定性错误，避免把他人误判为自己。
pub fn project_self(
    agents: &[AgentSummary],
    node_id: u64,
    workspace_id: &str,
) -> Result<AgentSelfInfo, ProjectSelfError> {
    if workspace_id.trim().is_empty() {
        return Err(ProjectSelfError::MissingContext);
    }
    let mut matches = agents
        .iter()
        .filter(|agent| agent.active_node_id == Some(node_id));
    let first = matches.next().ok_or(ProjectSelfError::NotFound)?;
    if matches.next().is_some() {
        return Err(ProjectSelfError::Ambiguous);
    }
    Ok(AgentSelfInfo {
        workspace_id: workspace_id.to_string(),
        definition_id: first.definition_id,
        node_id,
        name: first.name.clone(),
        speech_alias: first.speech_alias.clone(),
        profile: first.profile,
    })
}

fn render_self(format: OutputFormat, request_id: String, result: &CardCommandResult) -> CliOutput {
    let CardCommandResult::AgentsListed { agents } = result else {
        return render_error(
            format,
            request_id,
            &CardCommandError::agent(
                AgentErrorCode::InternalError,
                "agent self 收到了非 Agent 列表响应",
                false,
            ),
        );
    };
    let (node_id, workspace_id) = match self_process_context() {
        Ok(context) => context,
        Err(error) => return render_error(format, request_id, &error),
    };
    match project_self(agents, node_id, &workspace_id) {
        Ok(info) => render_agent_self_success(format, request_id, &info).unwrap_or_else(|error| {
            serialization_failure(format, request_id_for_failure(), error.to_string())
        }),
        Err(error) => render_error(format, request_id, &self_projection_error(error)),
    }
}

fn self_process_context() -> Result<(u64, String), CardCommandError> {
    let node_id = std::env::var("MULTISOUL_NODE_ID")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| {
            CardCommandError::agent(
                AgentErrorCode::SourceAgentUnavailable,
                "当前命令缺少有效的来源 Agent 节点",
                false,
            )
        })?;
    let workspace_id = std::env::var("NORMA_WORKSPACE_ID").ok().ok_or_else(|| {
        CardCommandError::agent(
            AgentErrorCode::SourceAgentUnavailable,
            "当前 Agent Session 缺少 Workspace 身份",
            false,
        )
    })?;
    Ok((node_id, workspace_id))
}

fn self_projection_error(error: ProjectSelfError) -> CardCommandError {
    match error {
        ProjectSelfError::NotFound => CardCommandError::agent(
            AgentErrorCode::SourceAgentUnavailable,
            "当前 Agent Session 没有对应的活跃 AgentCard",
            false,
        ),
        ProjectSelfError::Ambiguous => CardCommandError::agent(
            AgentErrorCode::InternalError,
            "当前 Agent Session 匹配到多个活跃 AgentCard，身份无法唯一确定",
            false,
        ),
        ProjectSelfError::MissingContext => CardCommandError::agent(
            AgentErrorCode::SourceAgentUnavailable,
            "当前 Agent Session 缺少 Workspace 身份",
            false,
        ),
    }
}

fn render_success(
    format: OutputFormat,
    request_id: String,
    result: &CardCommandResult,
) -> CliOutput {
    render_protocol_success(format, request_id, result).unwrap_or_else(|error| {
        serialization_failure(format, request_id_for_failure(), error.to_string())
    })
}

fn render_error(format: OutputFormat, request_id: String, error: &CardCommandError) -> CliOutput {
    render_protocol_error(format, request_id, error).unwrap_or_else(|serialization_error| {
        serialization_failure(
            format,
            request_id_for_failure(),
            serialization_error.to_string(),
        )
    })
}

fn serialization_failure(format: OutputFormat, request_id: String, message: String) -> CliOutput {
    match format {
        OutputFormat::Json => CliOutput {
            stdout: format!(
                "{{\"schema_version\":1,\"ok\":false,\"request_id\":\"{request_id}\",\
                 \"error\":{{\"code\":\"internal_error\",\"message\":\"JSON serialization failed\",\
                 \"retryable\":false}}}}"
            ),
            stderr: message,
            exit_code: 1,
        },
        OutputFormat::Text => CliOutput {
            stdout: String::new(),
            stderr: message,
            exit_code: 1,
        },
    }
}

fn local_request_id() -> String {
    request_id_for_failure()
}

fn request_id_for_failure() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("local-{}-{timestamp}", std::process::id())
}
