use crate::{
    parse_direct_address, DirectAddressIntent, NormaSessionError, NormaToolRuntime, ToolContext,
};
use serde_json::{json, Value};

/// 直接寻址产生的一次确定性工具调用。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectCall {
    pub tool: &'static str,
    pub arguments: Value,
}

/// 直接寻址的确定性决策：交给模型、直接答复、或由代码执行工具。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectPlan {
    Skip,
    Say(String),
    Calls(Vec<DirectCall>),
}

/// 纯函数：指令模式与豆包语音共用，匹配 Agent 名称或语音别名后由代码选择工具。
pub fn plan_direct_address(input: &str, roster: &[crate::AgentRosterEntry]) -> DirectPlan {
    match parse_direct_address(input, roster) {
        DirectAddressIntent::NoSideEffect => DirectPlan::Skip,
        DirectAddressIntent::RejectContextDependent { agents, reason, .. } => {
            let names = agents
                .iter()
                .map(|agent| agent.name.as_str())
                .collect::<Vec<_>>()
                .join("、");
            DirectPlan::Say(format!(
                "{names} 画布上还没有可续接的卡片，{reason}，所以我没有发送，也没有新建卡片。"
            ))
        }
        DirectAddressIntent::SessionReply { agents, message } => DirectPlan::Calls(
            agents
                .into_iter()
                .map(|agent| DirectCall {
                    tool: "reply_agent",
                    arguments: json!({"agent": agent.name, "message": message}),
                })
                .collect(),
        ),
        DirectAddressIntent::NewTask { agents, message }
        | DirectAddressIntent::ActivateAndAssign { agents, message } => DirectPlan::Calls(
            agents
                .into_iter()
                .map(|agent| DirectCall {
                    tool: "assign_agent_task",
                    arguments: json!({"agent": agent.name, "task": message}),
                })
                .collect(),
        ),
    }
}

/// 把单次工具回执转为口述文本。
pub fn direct_call_speech(tool: &str, message: &str, reply_status: Option<&str>) -> String {
    if tool != "reply_agent" || !message.trim().is_empty() {
        return message.trim().to_string();
    }
    match reply_status {
        Some("sent") => "已经发过去了。",
        Some("queued") => "我已经把这句话排为下一条。",
        Some("failed") => "没有发送成功。",
        _ => "处理完了。",
    }
    .to_string()
}

/// Run deterministic direct-address side effects before the model turn.
///
/// Returns `Some(final_text)` when the turn is fully handled without calling the model.
pub(crate) async fn maybe_handle_direct_address(
    tools: &NormaToolRuntime,
    context: &ToolContext,
    input: &str,
    roster: &[crate::AgentRosterEntry],
) -> Result<Option<String>, NormaSessionError> {
    if context.input_source == crate::AgentInputSource::GptLiveVoice {
        return Ok(None);
    }
    let calls = match plan_direct_address(input, roster) {
        DirectPlan::Skip => return Ok(None),
        DirectPlan::Say(text) => return Ok(Some(text)),
        DirectPlan::Calls(calls) => calls,
    };
    let mut parts = Vec::new();
    for (index, call) in calls.into_iter().enumerate() {
        let mut call_context = context.clone();
        let kind = if call.tool == "reply_agent" {
            "reply"
        } else {
            "assign"
        };
        call_context.tool_call_id = format!("{}:direct-{kind}-{index}", context.turn_id);
        call_context.operation_id = Some(call_context.tool_call_id.clone());
        let result = tools
            .execute(call.tool, call.arguments, &call_context)
            .await
            .map_err(map_adapter_error)?;
        if result.status == "unknown" {
            return Err(NormaSessionError::Runtime(
                "直接寻址操作结果待核对，已停止后续工具调用".into(),
            ));
        }
        parts.push(direct_call_speech(
            call.tool,
            &result.message,
            result.reply_status.as_deref(),
        ));
    }
    Ok(Some(join_spoken_parts(parts)))
}

fn join_spoken_parts(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("")
}

fn map_adapter_error(error: crate::AdapterError) -> NormaSessionError {
    match error {
        crate::AdapterError::Cancelled => NormaSessionError::Cancelled,
        other => NormaSessionError::Runtime(other.to_string()),
    }
}
