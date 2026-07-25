use crate::{
    parse_direct_address, AdapterResult, DirectAddressIntent, NormaSessionError, NormaToolRuntime,
    ToolContext,
};
use serde_json::json;

/// Run deterministic direct-address side effects before the model turn.
///
/// Returns `Some(final_text)` when the turn is fully handled without calling the model.
pub(crate) async fn maybe_handle_direct_address(
    tools: &NormaToolRuntime,
    context: &ToolContext,
    input: &str,
    roster: &[crate::AgentRosterEntry],
) -> Result<Option<String>, NormaSessionError> {
    match parse_direct_address(input, roster) {
        DirectAddressIntent::NoSideEffect => Ok(None),
        DirectAddressIntent::RejectContextDependent { agents, reason, .. } => {
            let names = agents
                .iter()
                .map(|agent| agent.name.as_str())
                .collect::<Vec<_>>()
                .join("、");
            Ok(Some(format!(
                "{names} 画布上还没有可续接的卡片，{reason}，所以我没有发送，也没有新建卡片。"
            )))
        }
        DirectAddressIntent::SessionReply { agents, message } => {
            let mut parts = Vec::new();
            for (index, agent) in agents.into_iter().enumerate() {
                let mut call_context = context.clone();
                call_context.tool_call_id = format!("{}:direct-reply-{index}", context.turn_id);
                call_context.operation_id = Some(call_context.tool_call_id.clone());
                let result = tools
                    .execute(
                        "reply_agent",
                        json!({
                            "agent": agent.name,
                            "message": message,
                        }),
                        &call_context,
                    )
                    .await
                    .map_err(map_adapter_error)?;
                parts.push(format_reply_result(&result));
            }
            Ok(Some(join_spoken_parts(parts)))
        }
        DirectAddressIntent::NewTask { agents, message }
        | DirectAddressIntent::ActivateAndAssign { agents, message } => {
            let mut parts = Vec::new();
            for (index, agent) in agents.into_iter().enumerate() {
                let mut call_context = context.clone();
                call_context.tool_call_id = format!("{}:direct-assign-{index}", context.turn_id);
                call_context.operation_id = Some(call_context.tool_call_id.clone());
                let result = tools
                    .execute(
                        "assign_agent_task",
                        json!({
                            "agent": agent.name,
                            "task": message,
                        }),
                        &call_context,
                    )
                    .await
                    .map_err(map_adapter_error)?;
                parts.push(result.message);
            }
            Ok(Some(join_spoken_parts(parts)))
        }
    }
}

fn join_spoken_parts(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("")
}

fn format_reply_result(result: &AdapterResult) -> String {
    if !result.message.trim().is_empty() {
        return result.message.clone();
    }
    match result.reply_status.as_deref() {
        Some("sent") => "已经发过去了。".to_string(),
        Some("queued") => "我已经把这句话排为下一条。".to_string(),
        Some("failed") => "没有发送成功。".to_string(),
        _ => "处理完了。".to_string(),
    }
}

fn map_adapter_error(error: crate::AdapterError) -> NormaSessionError {
    match error {
        crate::AdapterError::Cancelled => NormaSessionError::Cancelled,
        other => NormaSessionError::Runtime(other.to_string()),
    }
}
