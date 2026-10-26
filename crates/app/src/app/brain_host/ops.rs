//! `norma brain …` 命令的执行：播报、审批、读 Agent、派任务。

use super::{approvals, ask_key, AutoAction, AutoKind, BrainHostState, MultisoulApp, ASSIGN_KEY};
use gpui::{Context, Window};
use norma::mobile::ApprovalTarget;
use norma::protocol::{
    AgentErrorCode, AgentTaskAssignment, AgentTaskReceiptStatus, BrainDecision, BrainOp,
    CardCommand, CardCommandError, CardCommandResult,
};
use serde_json::json;

type OpResult = Result<CardCommandResult, CardCommandError>;

pub(super) fn reject(message: &str) -> CardCommandError {
    CardCommandError::agent(AgentErrorCode::InternalError, message, false)
}

fn output(value: serde_json::Value) -> OpResult {
    Ok(CardCommandResult::Brain {
        output: value.to_string(),
    })
}

impl MultisoulApp {
    /// 前台对话转来的请求：交给后台大脑。大脑没有运行时如实告知，不假装接收。
    pub(crate) fn handoff_to_brain(
        &mut self,
        request: String,
    ) -> Result<agent::AdapterResult, agent::AdapterError> {
        if self.brain.host.is_none() {
            return Err(agent::AdapterError::Rejected(
                "幕僚没有运行，无法交接".into(),
            ));
        }
        self.push_brain(brain::BrainEvent::UserRequest { text: request });
        Ok(agent::AdapterResult::accepted("已交给幕僚，结果会稍后播报"))
    }

    pub(crate) fn execute_brain_command(
        &mut self,
        token: &str,
        op: BrainOp,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> OpResult {
        if !self.brain.token_matches(token) {
            return Err(CardCommandError::Unsupported {
                message: "大脑令牌无效".into(),
            });
        }
        use agent::context::ContextOperation;
        let restriction = match &op {
            BrainOp::Assign { .. } => Some((ContextOperation::DispatchTask, None)),
            BrainOp::Decide {
                approval_id,
                decision: BrainDecision::Allow,
                ..
            } => Some((
                ContextOperation::ApproveOperation,
                self.brain
                    .tracked
                    .get(approval_id)
                    .map(|tracked| tracked.target.node_id.to_string()),
            )),
            _ => None,
        };
        if let Some((operation, target)) = restriction {
            let service = self
                .shared_context_service()
                .map_err(|e| reject(&e.to_string()))?;
            return service
                .execute(operation, target.as_deref(), || {
                    Ok(self.execute_brain_command_unchecked(op, window, cx))
                })
                .map_err(|e| reject(&e.to_string()))?;
        }
        self.execute_brain_command_unchecked(op, window, cx)
    }

    fn execute_brain_command_unchecked(
        &mut self,
        op: BrainOp,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> OpResult {
        match op {
            BrainOp::Context { .. } => Err(CardCommandError::Unsupported {
                message: "context 由端点异步处理".into(),
            }),
            BrainOp::Report { .. } => Err(reject("report 由端点异步保存后交付")),
            // 读对话要在后台完成，由端点异步处理，不会走到这里。
            BrainOp::History { .. } => Err(reject("history 由端点异步处理")),
            BrainOp::Approvals => output(self.approvals_json()),
            BrainOp::Decide {
                approval_id,
                decision,
                reason,
            } => self.decide_approval(&approval_id, decision, &reason, cx),
            BrainOp::Agents => output(self.agents_json()),
            BrainOp::Assign { agent, task } => {
                if !self.brain_autonomy().assign_followups {
                    return Err(reject(
                        "not_authorized：用户没有允许大脑自行派任务，请改用 propose 提议",
                    ));
                }
                self.assign_task(agent, task, window, AutoKind::Assigned)
            }
            BrainOp::Propose {
                agent,
                title,
                task,
                why,
            } => {
                let draft = super::proposals::Draft {
                    agent,
                    title,
                    task,
                    why,
                };
                let id = self.propose_next_step(draft, cx).map_err(|e| reject(&e))?;
                output(json!({"proposal": id, "status": "waiting_user"}))
            }
        }
    }

    fn approvals_json(&self) -> serde_json::Value {
        let items: Vec<_> = self
            .brain
            .tracked
            .iter()
            .map(|(id, t)| {
                json!({
                    "id": id,
                    "agent": t.agent,
                    "tool": t.subject.tool_name,
                    "summary": approvals::summarize(&t.subject),
                    "escalated": t.escalated,
                })
            })
            .collect();
        json!({ "approvals": items })
    }

    fn agents_json(&self) -> serde_json::Value {
        let items: Vec<_> = self
            .agent_roster_for_provider()
            .into_iter()
            .map(|a| {
                json!({
                    "name": a.name,
                    "profile": format!("{:?}", a.profile),
                    "enabled": a.enabled,
                    "busy": a.busy,
                    "state": format!("{:?}", a.task_state),
                    "running": a.active_node_id.is_some(),
                })
            })
            .collect();
        json!({ "agents": items })
    }

    pub(super) fn assign_task(
        &mut self,
        agent: String,
        task: String,
        window: Option<&mut Window>,
        kind: AutoKind,
    ) -> OpResult {
        let area = self.mobile_canvas_area(window);
        let zoom = self.runtime.document.viewport.zoom;
        let summary: String = task.chars().take(80).collect();
        let who = agent.clone();
        let command = CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: agent,
                task: task.clone(),
            }],
            force_untracked: false,
        };
        let result = crate::app::execute_high_level_agent_command(
            self,
            command,
            area,
            zoom,
            agent::AgentDispatchSource::norma(),
            None,
        )
        .map_err(|error| reject(&error.to_string()))?;
        let CardCommandResult::AgentTasksDispatched { receipts } = &result else {
            return Err(reject("派发没有返回任务回执"));
        };
        let [receipt] = receipts.as_slice() else {
            return Err(reject("派发回执数量不正确"));
        };
        if receipt.status == AgentTaskReceiptStatus::Failed {
            return Err(reject(&format!(
                "{} 未接收任务，请处理卡片后重试",
                receipt.name
            )));
        }
        self.brain.own_prompts.remember(&task);
        let decision_reason = if kind == AutoKind::UserDispatched {
            "你批准了幕僚的提议"
        } else {
            "幕僚按当前目标派发后续任务，宿主已通过执行校验"
        };
        self.brain.record_action(AutoAction {
            at: std::time::SystemTime::now(),
            kind,
            agent: who,
            node_id: receipt.node_id,
            summary,
            reason: decision_reason.into(),
            ask_key: ASSIGN_KEY.into(),
        });
        output(
            json!({"result": format!("{result:?}"), "receipt": receipt, "decision_reason": decision_reason}),
        )
    }

    fn decide_approval(
        &mut self,
        id: &str,
        decision: BrainDecision,
        reason: &str,
        cx: &mut Context<Self>,
    ) -> OpResult {
        let Some(tracked) = self.brain.tracked.get(id) else {
            return Err(reject("approval_closed：这条审批已经处理或失效"));
        };
        let target: ApprovalTarget = tracked.target.clone();
        let autonomy = self.brain_autonomy();
        let permitted = approvals::autonomy_allows(&autonomy, &tracked.subject.tool_name);
        let key = ask_key(&tracked.subject);
        let asked = self
            .brain
            .settings
            .as_ref()
            .is_some_and(|settings| settings.always_ask.contains(&key));
        let outcome = match decision {
            BrainDecision::Escalate => {
                self.escalate(id, reason, cx);
                return output(json!({"decision": "escalate"}));
            }
            // 用户没有勾选这类操作的自主权：交给用户。
            BrainDecision::Allow if !permitted => {
                self.escalate(id, "这类操作需要你确认", cx);
                return Err(reject(
                    "not_authorized：用户没有允许大脑自行放行这类操作，已转交用户",
                ));
            }
            BrainDecision::Allow if asked => {
                self.escalate(id, "你设置了这类先问你", cx);
                return Err(reject(
                    "not_authorized：用户要求这类操作先问用户，已转交用户",
                ));
            }
            BrainDecision::Allow => norma::agent_hook::ApprovalDecision::Allow,
            BrainDecision::Deny => norma::agent_hook::ApprovalDecision::Deny,
        };
        let applied = match target.profile.as_str() {
            "Claude" => self.runtime.decide_mobile_approval(
                target.node_id,
                &target.run_id,
                &target.request_id,
                outcome,
            ),
            "Codex" => self
                .runtime
                .decide_mobile_codex_approval(
                    target.node_id,
                    &target.run_id,
                    &target.request_id,
                    outcome,
                )
                .is_ok(),
            _ => false,
        };
        if !applied {
            return Err(reject("approval_closed：这条审批已经处理或失效"));
        }
        let removed = self.brain.tracked.remove(id);
        let summary = removed.as_ref().map(|t| approvals::summarize(&t.subject));
        self.brain
            .log_decision(id, decision, reason, summary.as_deref());
        if let Some(tracked) = removed {
            self.brain.record_action(AutoAction {
                at: std::time::SystemTime::now(),
                kind: if decision == BrainDecision::Allow {
                    AutoKind::Approved
                } else {
                    AutoKind::Denied
                },
                agent: tracked.agent,
                node_id: tracked.target.node_id,
                summary: approvals::summarize(&tracked.subject),
                reason: reason.to_string(),
                ask_key: ask_key(&tracked.subject),
            });
        }
        output(json!({"decision": format!("{decision:?}")}))
    }
}

impl BrainHostState {
    /// 追加一条决策记录，用户可以据此追问“为什么放行”。
    fn log_decision(&self, id: &str, decision: BrainDecision, reason: &str, summary: Option<&str>) {
        let Some(dir) = self.proposals_path.as_ref().and_then(|path| path.parent()) else {
            return;
        };
        let line = json!({"approval": id, "decision": format!("{decision:?}"), "reason": reason, "what": summary});
        self.proposal_writer
            .append(dir.join("approvals.jsonl"), line.to_string());
    }
}
