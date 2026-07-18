use super::ActiveTask;
use crate::AgentRosterEntry;
use crate::ProviderToolCall;
use norma::protocol::{AgentTaskReceiptStatus, CardCommand, CardCommandResult};

pub(super) enum DelegationCompletion {
    Succeeded(String),
    PartiallyDispatched(String),
    Failed(String),
}

pub(super) fn deterministic_delegation_call(
    task_id: u64,
    assignments: Option<&[norma::protocol::AgentTaskAssignment]>,
) -> Option<ProviderToolCall> {
    assignments.map(|assignments| ProviderToolCall {
        id: format!("delegation-dispatch-{task_id}"),
        name: "dispatch_agent_tasks".to_string(),
        arguments_json: serde_json::json!({
            "assignments": assignments,
            "force_untracked": false,
        })
        .to_string(),
    })
}

pub(super) fn delegation_completion(task: &ActiveTask) -> Option<DelegationCompletion> {
    let expected = task.delegation_assignments.as_ref()?;
    let receipts = task
        .successful_results
        .iter()
        .find_map(|result| match result {
            CardCommandResult::AgentTasksDispatched { receipts } => Some(receipts),
            _ => None,
        });
    let Some(receipts) = receipts else {
        if task.unresolved_tool_failures.is_empty() {
            return None;
        }
        let errors = task
            .unresolved_tool_failures
            .iter()
            .map(|failure| failure.error.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Some(DelegationCompletion::Failed(format!(
            "TaskIncomplete: deterministic dispatch failed: {errors}"
        )));
    };
    if let Err(error) = require_matching_delegation_receipts(expected, receipts) {
        return Some(DelegationCompletion::Failed(error));
    }
    let failed = receipts
        .iter()
        .filter(|receipt| receipt.status == AgentTaskReceiptStatus::Failed)
        .count();
    if failed == receipts.len() {
        return Some(DelegationCompletion::Failed(format!(
            "Agent task dispatch failed: {}",
            delegation_receipt_summary(receipts)
        )));
    }
    let summary = delegation_receipt_summary(receipts);
    if failed > 0 {
        Some(DelegationCompletion::PartiallyDispatched(summary))
    } else {
        Some(DelegationCompletion::Succeeded(summary))
    }
}

fn require_matching_delegation_receipts(
    expected: &[norma::protocol::AgentTaskAssignment],
    receipts: &[norma::protocol::AgentTaskReceipt],
) -> Result<(), String> {
    let mut matched_receipts = vec![false; receipts.len()];
    let mut missing = Vec::new();
    for assignment in expected {
        let match_index = receipts.iter().enumerate().find_map(|(index, receipt)| {
            (!matched_receipts[index] && names_match(&assignment.name, &receipt.name))
                .then_some(index)
        });
        if let Some(index) = match_index {
            matched_receipts[index] = true;
        } else {
            missing.push(assignment.name.as_str());
        }
    }
    let unexpected = receipts
        .iter()
        .zip(matched_receipts)
        .filter_map(|(receipt, matched)| (!matched).then_some(receipt.name.as_str()))
        .collect::<Vec<_>>();
    if missing.is_empty() && unexpected.is_empty() {
        return Ok(());
    }
    Err(format!(
        "TaskIncomplete: Agent dispatch receipt identity mismatch; missing=[{}]; unexpected=[{}]",
        missing.join(", "),
        unexpected.join(", ")
    ))
}

fn delegation_receipt_summary(receipts: &[norma::protocol::AgentTaskReceipt]) -> String {
    receipts
        .iter()
        .map(|receipt| {
            let result = match receipt.status {
                AgentTaskReceiptStatus::Submitted => "已派发",
                AgentTaskReceiptStatus::Queued => "正在执行",
                AgentTaskReceiptStatus::Untracked => "已派发（无法跟踪）",
                AgentTaskReceiptStatus::Failed => "派发失败",
            };
            format!("{} {result}", receipt.name)
        })
        .collect::<Vec<_>>()
        .join("；")
}

pub(super) fn require_delegation_targets(
    task: &ActiveTask,
    roster: &[AgentRosterEntry],
    command: &CardCommand,
) -> Result<(), String> {
    let Some(expected_assignments) = task.delegation_assignments.as_ref() else {
        return Ok(());
    };
    let expected = expected_assignments
        .iter()
        .map(|assignment| assignment.name.as_str())
        .collect::<Vec<_>>();
    match command {
        CardCommand::DispatchAgentTasks { assignments, .. } => {
            let resolved = assignments
                .iter()
                .map(|assignment| resolve_canonical_name(&assignment.name, roster))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    "DelegationTargetMismatch: assignment contains an unknown addressed Agent"
                        .to_string()
                })?;
            let unique_resolved = resolved.iter().fold(Vec::<&str>::new(), |mut names, name| {
                if !names.contains(name) {
                    names.push(name);
                }
                names
            });
            if assignments.len() != expected.len()
                || unique_resolved.len() != expected.len()
                || expected.iter().any(|name| !unique_resolved.contains(name))
            {
                return Err(format!(
                    "DelegationTargetMismatch: expected exactly [{}] from the user's addressed set",
                    expected.join(", ")
                ));
            }
            Ok(())
        }
        CardCommand::ActivateAgent { name } => {
            let canonical = resolve_canonical_name(name, roster).ok_or_else(|| {
                format!("DelegationTargetMismatch: cannot activate unaddressed Agent {name}")
            })?;
            if expected.contains(&canonical) {
                Ok(())
            } else {
                Err(format!(
                    "DelegationTargetMismatch: cannot activate unaddressed Agent {name}"
                ))
            }
        }
        CardCommand::CreateAgent { .. } => Err(
            "DelegationTargetMismatch: direct address must not create another Agent".to_string(),
        ),
        _ => Ok(()),
    }
}

fn resolve_canonical_name<'a>(name: &str, roster: &'a [AgentRosterEntry]) -> Option<&'a str> {
    roster.iter().find_map(|entry| {
        (names_match(name, &entry.name)
            || entry
                .speech_alias
                .as_deref()
                .is_some_and(|alias| names_match(name, alias)))
        .then_some(entry.name.as_str())
    })
}

fn names_match(left: &str, right: &str) -> bool {
    if left.is_ascii() && right.is_ascii() {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

pub(super) fn require_verified_target(
    task: &ActiveTask,
    command: &CardCommand,
) -> Result<(), String> {
    let node_id = match command {
        CardCommand::Delete { node_id }
        | CardCommand::SendText { node_id, .. }
        | CardCommand::Focus { node_id } => *node_id,
        CardCommand::Create { .. }
        | CardCommand::List
        | CardCommand::ListAgents
        | CardCommand::CreateAgent { .. }
        | CardCommand::ActivateAgent { .. }
        | CardCommand::DispatchAgentTasks { .. } => return Ok(()),
    };
    if task.created_node_ids.contains(&node_id) || task.verified_node_ids.contains(&node_id) {
        return Ok(());
    }
    Err(format!(
        "TargetUnverified: node_id={node_id} requires a successful list_cards receipt or a Created receipt from this task"
    ))
}

pub(super) fn same_recoverable_operation(
    failed: &CardCommand,
    failed_error: &str,
    succeeded: &CardCommand,
) -> bool {
    failed == succeeded
        || (failed_error.contains("DelegationTargetMismatch")
            && matches!(
                (failed, succeeded),
                (
                    CardCommand::DispatchAgentTasks { .. },
                    CardCommand::DispatchAgentTasks { .. }
                )
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
        CardCommandResult::AgentCreated {
            node_id: Some(node_id),
            ..
        }
        | CardCommandResult::AgentActivated { node_id, .. } => {
            task.created_node_ids.insert(*node_id);
        }
        CardCommandResult::Sent { .. }
        | CardCommandResult::Focused { .. }
        | CardCommandResult::AgentsListed { .. }
        | CardCommandResult::AgentCreated { node_id: None, .. }
        | CardCommandResult::AgentTasksDispatched { .. } => {}
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
    let agent_created = task
        .successful_results
        .iter()
        .any(|result| matches!(result, CardCommandResult::AgentCreated { .. }));
    let agent_activated = task
        .successful_results
        .iter()
        .any(|result| matches!(result, CardCommandResult::AgentActivated { .. }));
    let agent_dispatch_counts = agent_dispatch_counts(&task.successful_results);

    if let Some((0, _)) = agent_dispatch_counts {
        "Agent 任务提交失败"
    } else if let Some((_, failed)) = agent_dispatch_counts.filter(|(_, failed)| *failed > 0) {
        return format!("Agent 任务部分提交或排队（{failed} 项失败）");
    } else if agent_dispatch_counts.is_some() {
        "Agent 任务已提交或排队"
    } else if agent_created {
        "已创建 Agent"
    } else if agent_activated {
        "已激活 Agent"
    } else if deleted {
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

fn agent_dispatch_counts(results: &[CardCommandResult]) -> Option<(usize, usize)> {
    results
        .iter()
        .filter_map(|result| match result {
            CardCommandResult::AgentTasksDispatched { receipts } => Some(receipts),
            _ => None,
        })
        .fold(None, |counts, receipts| {
            let failed = receipts
                .iter()
                .filter(|receipt| receipt.status == AgentTaskReceiptStatus::Failed)
                .count();
            let (accepted_total, failed_total) = counts.unwrap_or((0, 0));
            Some((
                accepted_total + receipts.len() - failed,
                failed_total + failed,
            ))
        })
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
            CardCommandResult::AgentsListed { agents } => {
                format!("列出 {} 个 Agent", agents.len())
            }
            CardCommandResult::AgentCreated { name, node_id, .. } => node_id
                .map(|node_id| format!("创建 Agent {name}（卡片 #{node_id}）"))
                .unwrap_or_else(|| format!("创建 Agent {name}")),
            CardCommandResult::AgentActivated { name, node_id, .. } => {
                format!("激活 Agent {name}（卡片 #{node_id}）")
            }
            CardCommandResult::AgentTasksDispatched { receipts } => {
                let failed = receipts
                    .iter()
                    .filter(|receipt| receipt.status == AgentTaskReceiptStatus::Failed)
                    .count();
                let accepted = receipts.len() - failed;
                match (accepted, failed) {
                    (0, failed) => format!("Agent 任务提交失败 {failed} 项"),
                    (accepted, 0) => format!("提交或排队 {accepted} 项 Agent 任务"),
                    (accepted, failed) => {
                        format!("提交或排队 {accepted} 项 Agent 任务，失败 {failed} 项")
                    }
                }
            }
        })
        .collect::<Vec<_>>()
        .join("；")
}

#[cfg(test)]
mod tests {
    use super::*;
    use norma::protocol::AgentTaskReceipt;

    /// 多次 Agent 派发必须汇总全部回执，不能忽略后续失败。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   dispatch results = 2 批：首批 Submitted 1 项，次批 Failed 1 项
    ///   expected totals  = accepted 1 + failed 1
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 构造同一 CardTask 的两次 AgentTasksDispatched 结果；
    ///   2. 调用摘要计数 helper 遍历全部 successful_results；
    ///   3. 比较汇总后的接受数与失败数。
    ///
    /// 预期结果：
    ///   - 正断言：accepted=1、failed=1；
    ///   - 负断言：不得只读取首批而得到 failed=0。
    #[test]
    fn agent_dispatch_counts_aggregates_later_failures() {
        let receipt = |status| AgentTaskReceipt {
            name: "Alan".to_string(),
            node_id: 1,
            task: "任务".to_string(),
            status,
        };
        let results = vec![
            CardCommandResult::AgentTasksDispatched {
                receipts: vec![receipt(AgentTaskReceiptStatus::Submitted)],
            },
            CardCommandResult::AgentTasksDispatched {
                receipts: vec![receipt(AgentTaskReceiptStatus::Failed)],
            },
        ];

        let counts = agent_dispatch_counts(&results);

        assert_eq!(counts, Some((1, 1)), "摘要计数必须汇总首批成功与后续失败");
        assert_ne!(counts, Some((1, 0)), "摘要计数不得在首个派发结果后提前停止");
    }
}
