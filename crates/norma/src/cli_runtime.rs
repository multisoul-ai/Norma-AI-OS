use crate::cli::{
    render_protocol_error, render_protocol_success, CliInvocation, CliOutput, OutputFormat,
};
use crate::client;
use crate::protocol::{
    AgentErrorCode, AgentTaskReceiptStatus, CardCommandError, CardCommandResult,
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

pub fn run_invocation(invocation: CliInvocation) -> CliOutput {
    match invocation.wait_timeout_seconds {
        Some(timeout_seconds) => run_wait(invocation, Duration::from_secs(timeout_seconds)),
        None => run_once(invocation),
    }
}

fn run_once(invocation: CliInvocation) -> CliOutput {
    match client::send_command_with_metadata(invocation.command) {
        Ok(response) => render_success(
            invocation.format,
            response.request_id.to_string(),
            &response.result,
        ),
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
        let response = match client::send_command_with_metadata(invocation.command.clone()) {
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
        match classify_wait_status(task.status) {
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
