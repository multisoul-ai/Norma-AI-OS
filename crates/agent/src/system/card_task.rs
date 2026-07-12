use super::ActiveTask;
use norma::protocol::{CardCommand, CardCommandResult};

pub(super) fn require_verified_target(
    task: &ActiveTask,
    command: &CardCommand,
) -> Result<(), String> {
    let node_id = match command {
        CardCommand::Delete { node_id }
        | CardCommand::SendText { node_id, .. }
        | CardCommand::Focus { node_id } => *node_id,
        CardCommand::Create { .. } | CardCommand::List => return Ok(()),
    };
    if task.created_node_ids.contains(&node_id) || task.verified_node_ids.contains(&node_id) {
        return Ok(());
    }
    Err(format!(
        "TargetUnverified: node_id={node_id} requires a successful list_cards receipt or a Created receipt from this task"
    ))
}

pub(super) fn record_successful_result(task: &mut ActiveTask, result: &CardCommandResult) {
    match result {
        CardCommandResult::Created { node_id } => {
            task.created_node_ids.insert(*node_id);
        }
        CardCommandResult::Deleted { node_id } => {
            task.created_node_ids.remove(node_id);
            task.verified_node_ids.remove(node_id);
        }
        CardCommandResult::Listed { cards } => {
            task.verified_node_ids.clear();
            task.verified_node_ids
                .extend(cards.iter().map(|card| card.node_id));
        }
        CardCommandResult::Sent { .. } | CardCommandResult::Focused { .. } => {}
    }
    task.successful_results.push(result.clone());
}

pub(super) fn successful_summary(task: &ActiveTask) -> String {
    let created = task
        .successful_results
        .iter()
        .any(|result| matches!(result, CardCommandResult::Created { .. }));
    let submitted = task
        .successful_results
        .iter()
        .any(|result| matches!(result, CardCommandResult::Sent { .. }));
    let deleted = task
        .successful_results
        .iter()
        .any(|result| matches!(result, CardCommandResult::Deleted { .. }));
    let focused = task
        .successful_results
        .iter()
        .any(|result| matches!(result, CardCommandResult::Focused { .. }));
    let listed = task
        .successful_results
        .iter()
        .any(|result| matches!(result, CardCommandResult::Listed { .. }));

    if deleted {
        "已删除卡片"
    } else if created && submitted {
        "已创建并提交"
    } else if created {
        "已创建卡片"
    } else if submitted {
        "已提交内容"
    } else if focused {
        "已聚焦卡片"
    } else if listed {
        "已获取卡片"
    } else {
        "已完成操作"
    }
    .to_string()
}

pub(crate) fn failed_summary(task: &ActiveTask, reason: &str) -> String {
    let operations = successful_operations(task);
    if operations.is_empty() {
        format!("未完成：{reason}")
    } else {
        format!("未完成：{reason}；已完成操作：{operations}")
    }
}

fn successful_operations(task: &ActiveTask) -> String {
    task.successful_results
        .iter()
        .map(|result| match result {
            CardCommandResult::Created { node_id } => format!("创建卡片 #{node_id}"),
            CardCommandResult::Deleted { node_id } => format!("删除卡片 #{node_id}"),
            CardCommandResult::Sent { node_id } => format!("向卡片 #{node_id} 提交文本"),
            CardCommandResult::Focused { node_id } => format!("聚焦卡片 #{node_id}"),
            CardCommandResult::Listed { cards } => format!("列出 {} 张卡片", cards.len()),
        })
        .collect::<Vec<_>>()
        .join("；")
}
