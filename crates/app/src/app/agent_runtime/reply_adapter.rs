use crate::app::MultisoulApp;
use agent::{AdapterCommand, AdapterError, AdapterResult};

pub(super) fn execute_reply_adapter_command(
    app: &mut MultisoulApp,
    command: AdapterCommand,
) -> Result<AdapterResult, AdapterError> {
    match command {
        AdapterCommand::ReplyAgent {
            agent,
            message,
            node_id,
            ..
        } => reply_agent(app, agent, message, node_id),
        AdapterCommand::CancelAgentReply { reply_id, agent } => {
            cancel_agent_reply(app, reply_id, agent)
        }
        other => Err(AdapterError::Failed(format!(
            "reply adapter received unexpected command: {other:?}"
        ))),
    }
}

fn reply_agent(
    app: &mut MultisoulApp,
    agent: String,
    message: String,
    target_node_id: Option<u64>,
) -> Result<AdapterResult, AdapterError> {
    let definition = app
        .agent_definitions
        .definition_by_address(&agent)
        .ok_or_else(|| AdapterError::Rejected(format!("未找到 Agent：{agent}")))?;
    if !definition.enabled {
        return Err(AdapterError::Rejected(format!("Agent 已禁用：{agent}")));
    }
    let definition_id = definition.id;
    let name = definition.name.clone();
    let node_id = target_node_id.or_else(|| app
        .runtime
        .document
        .nodes()
        .iter()
        .find_map(|node| {
            let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            (spec.definition_id == Some(definition_id)).then_some(node.id)
        }))
        .ok_or_else(|| {
            AdapterError::Rejected(format!(
                "{name} 画布上还没有卡片；若定义已启用，请用 assign_agent_task 自动打开唯一卡片后再对话"
            ))
        })?;
    // Card presence is enough to converse. Session id may arrive later via Hook;
    // an empty id means "bound to this card, not a specific provider session yet".
    let session_id = app.runtime.agent_session_id(node_id).unwrap_or_default();
    let receipt = app
        .enqueue_or_send_session_reply(definition_id, name.clone(), node_id, session_id, message)
        .map_err(AdapterError::Failed)?;
    let status = receipt.status.as_str();
    let text = match receipt.status {
        agent::AgentReplyStatus::Sent => format!("已经发给 {name} 了。"),
        agent::AgentReplyStatus::Queued => {
            format!("{name} 的输入暂不可用，这条消息等待发送。")
        }
        agent::AgentReplyStatus::Failed => {
            format!("没能发给 {name}。")
        }
        agent::AgentReplyStatus::Cancelled => format!("还没发出的 {name} 回复已经撤回。"),
        agent::AgentReplyStatus::Interrupted => format!("{name} 的未发送回复已中断。"),
    };
    Ok(AdapterResult::succeeded(text)
        .with_node_id(node_id)
        .with_reply_id(receipt.reply_id)
        .with_reply_status(status))
}

fn cancel_agent_reply(
    app: &mut MultisoulApp,
    reply_id: Option<u64>,
    agent: Option<String>,
) -> Result<AdapterResult, AdapterError> {
    if let Some(reply_id) = reply_id {
        let Some(peeked) = app.session_reply_queue.peek_reply(reply_id).cloned() else {
            return Err(AdapterError::Rejected(format!(
                "未找到可撤回的回复 #{reply_id}"
            )));
        };
        if let Some(expected) = agent.as_deref() {
            if !peeked.name.eq_ignore_ascii_case(expected.trim()) {
                return Err(AdapterError::Rejected(
                    "reply_id 不属于指定的 Agent".to_string(),
                ));
            }
        }
        let Some(receipt) = app.session_reply_queue.cancel_reply_id(reply_id) else {
            return Err(AdapterError::Rejected(format!(
                "回复 #{reply_id} 已提交，无法撤回"
            )));
        };
        return Ok(
            AdapterResult::succeeded(format!("还没发出的 {} 回复已经撤回。", receipt.name))
                .with_reply_id(receipt.reply_id)
                .with_reply_status(agent::AgentReplyStatus::Cancelled.as_str()),
        );
    }
    let agent = agent.ok_or_else(|| {
        AdapterError::InvalidArguments("cancel_agent_reply requires reply_id or agent".to_string())
    })?;
    let cancelled = app.session_reply_queue.cancel_agent(&agent);
    if cancelled.is_empty() {
        return Err(AdapterError::Rejected(format!("没有可撤回的 {agent} 回复")));
    }
    Ok(
        AdapterResult::succeeded(format!("还没发出的 {agent} 回复已经撤回。"))
            .with_reply_status(agent::AgentReplyStatus::Cancelled.as_str()),
    )
}
