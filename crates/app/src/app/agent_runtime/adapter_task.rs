use crate::app::MultisoulApp;
use agent::{AdapterError, AdapterResult};
use gpui::{Context, Window};
use norma::protocol::{
    AgentTaskAssignment, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};

use super::commands;

pub(super) fn dispatch_agent_task(
    app: &mut MultisoulApp,
    window: Option<&mut Window>,
    cx: &mut Context<MultisoulApp>,
    agent: String,
    task: String,
    force_untracked: bool,
    manual: bool,
) -> Result<AdapterResult, AdapterError> {
    let definition_id = app
        .agent_definitions
        .definition_by_address(&agent)
        .map(|definition| definition.id)
        .ok_or_else(|| AdapterError::Rejected(format!("未找到 Agent：{agent}")))?;
    // 用户亲手派发（拖拽想法）按“手动”来源入队，不再叠加 Norma 指令栏的飞行效果。
    let source = if manual {
        agent::AgentDispatchSource::manual()
    } else {
        agent::AgentDispatchSource::norma()
    };
    let result = commands::execute_agent_card_command_with_source(
        app,
        window,
        CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: agent,
                task: task.clone(),
            }],
            force_untracked,
        },
        source,
    )
    .map_err(AdapterError::Rejected)?;
    let CardCommandResult::AgentTasksDispatched { receipts } = result else {
        return Err(AdapterError::Failed(
            "Agent 派发没有返回任务回执".to_string(),
        ));
    };
    let receipt = receipts
        .first()
        .ok_or_else(|| AdapterError::Failed("Agent 派发回执为空".to_string()))?;
    if matches!(receipt.status, AgentTaskReceiptStatus::Failed) {
        return Err(AdapterError::Rejected(format!(
            "{} 未接收任务",
            receipt.name
        )));
    }
    let task_id = app
        .agent_task_coordinator
        .as_ref()
        .and_then(|coordinator| coordinator.task_id_for(definition_id, &task))
        .ok_or_else(|| AdapterError::Failed("AgentCard 已接收但无法确定 task_id".to_string()))?;
    cx.notify();
    Ok(
        AdapterResult::accepted(format!("{} 的 AgentCard 已接收任务", receipt.name))
            .with_task_id(task_id)
            .with_node_id(receipt.node_id),
    )
}
