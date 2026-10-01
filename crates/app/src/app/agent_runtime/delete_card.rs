use crate::app::MultisoulApp;
use agent::{AdapterError, AdapterResult};
use gpui::Window;
use norma::protocol::CardCommand;

pub(super) fn close_node(
    app: &mut MultisoulApp,
    window: Option<&mut Window>,
    node_id: u64,
    success_message: &str,
    require_repeat: bool,
) -> Result<AdapterResult, AdapterError> {
    let title = app
        .runtime
        .document
        .node(node_id)
        .map(|node| node.kind.display_title())
        .ok_or_else(|| AdapterError::Rejected(format!("未找到卡片 #{node_id}")))?;
    if !app.confirm_agent_close_if_needed(node_id, require_repeat) {
        return Err(AdapterError::Rejected(format!(
            "{title} 正在工作或有排队任务；请再次明确关闭以中断全部任务"
        )));
    }
    super::map_card_result(
        super::commands::execute_agent_card_command_with_optional_window(
            app,
            window,
            CardCommand::Delete { node_id },
        ),
        success_message,
    )
}

pub(super) fn close_position(
    app: &mut MultisoulApp,
    window: Option<&mut Window>,
    position: u64,
    require_repeat: bool,
) -> Result<AdapterResult, AdapterError> {
    let node_id = position
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| app.runtime.document.nodes().get(index))
        .map(|node| node.id)
        .ok_or_else(|| AdapterError::Rejected(format!("画布中不存在第 {position} 张卡片")))?;
    close_node(app, window, node_id, "已删除卡片", require_repeat)
}

pub(super) fn close_agent(
    app: &mut MultisoulApp,
    window: Option<&mut Window>,
    agent: &str,
    require_repeat: bool,
) -> Result<AdapterResult, AdapterError> {
    let definition_id = app
        .agent_definitions
        .definition_by_address(agent)
        .map(|definition| definition.id)
        .ok_or_else(|| AdapterError::Rejected(format!("未找到 Agent：{agent}")))?;
    let node_id = app
        .runtime
        .document
        .nodes()
        .iter()
        .find_map(|node| {
            let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
                return None;
            };
            (spec.definition_id == Some(definition_id)).then_some(node.id)
        })
        .ok_or_else(|| AdapterError::Rejected(format!("Agent {agent} 当前没有活跃卡片")))?;
    close_node(app, window, node_id, "已关闭 Agent 卡片", require_repeat)
}
