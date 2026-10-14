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
    target_node_id: Option<u64>,
) -> Result<AdapterResult, AdapterError> {
    app.agent_definitions
        .definition_by_address(&agent)
        .map(|definition| definition.id)
        .ok_or_else(|| AdapterError::Rejected(format!("未找到 Agent：{agent}")))?;
    let command = CardCommand::DispatchAgentTasks {
        assignments: vec![AgentTaskAssignment {
            name: agent,
            task: task.clone(),
        }],
        force_untracked,
    };
    let result = if let Some(node_id) = target_node_id {
        let area = app.mobile_canvas_area(window);
        let zoom = app.runtime.document.viewport.zoom;
        commands::execute_coordinated_agent_command(
            app,
            command,
            area,
            zoom,
            agent::AgentDispatchSource::norma(),
            None,
            commands::DispatchOptions {
                target_node_id: Some(node_id),
                visual_origin: None,
            },
        )
        .map_err(|e| AdapterError::Rejected(e.to_string()))?
    } else {
        commands::execute_agent_card_command_with_optional_window(app, window, command)
            .map_err(AdapterError::Rejected)?
    };
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
        .and_then(|coordinator| {
            coordinator
                .task_by_ref(&receipt.task_id)
                .ok()
                .map(|t| t.task_id)
        })
        .ok_or_else(|| AdapterError::Failed("AgentCard 已接收但无法确定 task_id".to_string()))?;
    cx.notify();
    Ok(
        AdapterResult::accepted(format!("{} 的 AgentCard 已接收任务", receipt.name))
            .with_task_id(task_id)
            .with_node_id(receipt.node_id),
    )
}
