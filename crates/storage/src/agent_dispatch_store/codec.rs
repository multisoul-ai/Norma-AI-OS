use super::{
    AgentDispatchTaskState, AgentDispatchTracking, StoredAgentDispatchResult,
    StoredAgentDispatchTask,
};
use crate::StorageError;

pub(super) fn decode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredAgentDispatchTask> {
    let state: String = row.get(13)?;
    let tracking: String = row.get(14)?;
    let result_captured = row.get::<_, i64>(16)? != 0;
    let result_text: Option<String> = row.get(17)?;
    let result_truncated = row.get::<_, i64>(18)? != 0;
    // result_background_unsettled 是 SELECT 的第 20 列，下标 19。
    let background_unsettled = row.get::<_, i64>(19)? != 0;
    Ok(StoredAgentDispatchTask {
        id: row.get(0)?,
        runtime_instance_id: row.get(1)?,
        runtime_task_id: row.get(2)?,
        task_ref: row.get(3)?,
        root_task_ref: row.get(4)?,
        parent_task_ref: row.get(5)?,
        source_definition_id: row.get(6)?,
        source_node_id: row.get(7)?,
        source_provider: row.get(8)?,
        definition_id: row.get(9)?,
        node_id: row.get(10)?,
        original_instruction: row.get(11)?,
        task_text: row.get(12)?,
        state: decode_state(&state)?,
        tracking: decode_tracking(&tracking)?,
        error: row.get(15)?,
        result: result_captured.then_some(StoredAgentDispatchResult {
            text: result_text,
            truncated: result_truncated,
            background_unsettled,
        }),
    })
}

pub(super) fn encode_state(state: AgentDispatchTaskState) -> &'static str {
    match state {
        AgentDispatchTaskState::Accepted => "accepted",
        AgentDispatchTaskState::Submitted => "submitted",
        AgentDispatchTaskState::Queued => "queued",
        AgentDispatchTaskState::Untracked => "untracked",
        AgentDispatchTaskState::Done => "done",
        AgentDispatchTaskState::Failed => "failed",
        AgentDispatchTaskState::CancelRequested => "cancel_requested",
        AgentDispatchTaskState::Cancelled => "cancelled",
        AgentDispatchTaskState::Interrupted => "interrupted",
    }
}

fn decode_state(value: &str) -> rusqlite::Result<AgentDispatchTaskState> {
    match value {
        "accepted" => Ok(AgentDispatchTaskState::Accepted),
        "submitted" => Ok(AgentDispatchTaskState::Submitted),
        "queued" => Ok(AgentDispatchTaskState::Queued),
        "untracked" => Ok(AgentDispatchTaskState::Untracked),
        "done" => Ok(AgentDispatchTaskState::Done),
        "failed" => Ok(AgentDispatchTaskState::Failed),
        "cancel_requested" => Ok(AgentDispatchTaskState::CancelRequested),
        "cancelled" => Ok(AgentDispatchTaskState::Cancelled),
        "interrupted" => Ok(AgentDispatchTaskState::Interrupted),
        other => Err(invalid_text(other)),
    }
}

pub(super) fn encode_tracking(tracking: AgentDispatchTracking) -> &'static str {
    match tracking {
        AgentDispatchTracking::Managed => "managed",
        AgentDispatchTracking::Untracked => "untracked",
    }
}

fn decode_tracking(value: &str) -> rusqlite::Result<AgentDispatchTracking> {
    match value {
        "managed" => Ok(AgentDispatchTracking::Managed),
        "untracked" => Ok(AgentDispatchTracking::Untracked),
        other => Err(invalid_text(other)),
    }
}

fn invalid_text(value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        value.len(),
        rusqlite::types::Type::Text,
        Box::new(StorageError::InvalidAgentDispatchValue(value.to_string())),
    )
}
