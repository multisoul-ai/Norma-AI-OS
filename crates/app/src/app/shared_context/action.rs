//! Persist evidence after host acceptance. A failed context write never turns an
//! accepted action into a retryable action error, and never re-executes the action.
use super::*;
use norma::protocol::{
    AgentTaskReceipt, AgentTaskReceiptStatus, BrainDecision, BrainOp, CardCommandResult,
};
use serde_json::json;
use sha2::{Digest, Sha256};

impl SharedContext {
    pub(crate) async fn save_brain_action_result(
        &self,
        op: BrainOp,
        result: &mut CardCommandResult,
    ) -> bool {
        let CardCommandResult::Brain { output } = result else {
            return false;
        };
        if !matches!(
            op,
            BrainOp::Assign { .. }
                | BrainOp::Decide {
                    decision: BrainDecision::Allow | BrainDecision::Deny,
                    ..
                }
        ) {
            return false;
        }
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(output) else {
            return false;
        };
        let saved = async {
            let fact = ActionFact::from_result(op, &value)?;
            let service = self.clone();
            // The runtime owns the write even when the CLI waiter times out or disappears.
            super::super::agent_runtime::shared_tokio_runtime()
                .map_err(AdapterError::Failed)?
                .spawn(async move {
                    tokio::time::timeout(
                        std::time::Duration::from_secs(2),
                        service.run(None, move |access| {
                            let decision = access.host_event(
                                &format!("{}:decision", fact.identity),
                                ContextPayload::Decision {
                                    status: fact.decision,
                                    reason: fact.reason.clone(),
                                },
                                vec![],
                            )?;
                            let receipt = access.host_event(
                                &format!("{}:receipt", fact.identity),
                                ContextPayload::Receipt {
                                    operation_id: fact.operation_id.clone(),
                                    status: fact.status,
                                    summary: fact.summary.clone(),
                                },
                                vec![decision.reference.clone()],
                            )?;
                            Ok(vec![decision.reference, receipt.reference])
                        }),
                    )
                    .await
                    .map_err(|_| AdapterError::Failed("context_save_pending".into()))?
                })
                .await
                .map_err(failed)?
        }
        .await;
        let succeeded = saved.is_ok();
        value["action_accepted"] = json!(true);
        value["context"] = match saved {
            Ok(sources) => json!({"saved":true,"sources":sources}),
            Err(error) => save_failure(error.to_string()),
        };
        *output = value.to_string();
        succeeded
    }
}

struct ActionFact {
    identity: String,
    operation_id: String,
    decision: DecisionStatus,
    reason: String,
    status: ReceiptStatus,
    summary: String,
}

impl ActionFact {
    fn from_result(op: BrainOp, value: &serde_json::Value) -> Result<Self, AdapterError> {
        match op {
            BrainOp::Assign { task, .. } => {
                let receipt: AgentTaskReceipt =
                    serde_json::from_value(value["receipt"].clone()).map_err(failed)?;
                let status = match receipt.status {
                    AgentTaskReceiptStatus::Done => ReceiptStatus::Succeeded,
                    AgentTaskReceiptStatus::Failed => ReceiptStatus::Failed,
                    AgentTaskReceiptStatus::Cancelled | AgentTaskReceiptStatus::Interrupted => {
                        ReceiptStatus::Unknown
                    }
                    _ => ReceiptStatus::Accepted,
                };
                Ok(Self {
                    identity: format!("assignment:{}", receipt.task_id),
                    operation_id: receipt.task_id,
                    decision: DecisionStatus::Allowed,
                    reason: value["decision_reason"]
                        .as_str()
                        .unwrap_or("宿主已通过派发执行校验")
                        .into(),
                    status,
                    summary: format!(
                        "任务派发回执：{}；目标 {}；任务：{}",
                        value["receipt"],
                        receipt.name,
                        excerpt(&task, 4096)
                    ),
                })
            }
            BrainOp::Decide {
                approval_id,
                decision,
                reason,
            } if value["decision"] == format!("{decision:?}") => {
                let status = match decision {
                    BrainDecision::Allow => DecisionStatus::Allowed,
                    BrainDecision::Deny => DecisionStatus::Denied,
                    BrainDecision::Escalate => return Err(failed("unexpected_approval_result")),
                };
                let original = format!("approval:{approval_id}");
                let operation_id = if original.len() <= 256 {
                    original.clone()
                } else {
                    format!("approval-sha256:{:x}", Sha256::digest(original.as_bytes()))
                };
                Ok(Self {
                    identity: operation_id.clone(), operation_id, decision: status,
                    reason: if reason.trim().is_empty() { "未提供理由".into() } else { excerpt(&reason, 8000) },
                    status: ReceiptStatus::Accepted,
                    summary: format!("审批选择 {decision:?} 已由 Runtime 接收；{original}；这不表示所请求操作执行完成"),
                })
            }
            _ => Err(failed("unexpected_brain_action_result")),
        }
    }
}

fn save_failure(error: String) -> serde_json::Value {
    json!({"saved":false,"error":error,"retry_action":false,
        "message":"动作已接收；来源尚未确认保存，请勿重复执行动作"})
}

pub(crate) fn brain_action_context_failure(result: &mut CardCommandResult, error: String) {
    if let CardCommandResult::Brain { output } = result {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(output) {
            value["action_accepted"] = json!(true);
            value["context"] = save_failure(error);
            *output = value.to_string();
        }
    }
}

fn excerpt(text: &str, bytes: usize) -> String {
    if text.len() <= bytes {
        return text.into();
    }
    let mut end = bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…（摘要截断）", &text[..end])
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_decision_context_tests.rs"]
mod tests;
