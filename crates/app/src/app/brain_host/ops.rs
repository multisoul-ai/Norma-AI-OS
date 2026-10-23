//! `norma brain …` 命令的执行：播报、审批、读 Agent、派任务。

use super::{approvals, BrainHostState, MultisoulApp};
use brain::risk::{risk_class, RiskClass};
use gpui::{Context, Window};
use norma::mobile::ApprovalTarget;
use norma::protocol::{
    AgentErrorCode, AgentTaskAssignment, BrainDecision, BrainOp, CardCommand, CardCommandError,
    CardCommandResult,
};
use serde_json::json;
use std::io::Write;

type OpResult = Result<CardCommandResult, CardCommandError>;

fn reject(message: &str) -> CardCommandError {
    CardCommandError::agent(AgentErrorCode::InternalError, message, false)
}

fn output(value: serde_json::Value) -> OpResult {
    Ok(CardCommandResult::Brain {
        output: value.to_string(),
    })
}

impl MultisoulApp {
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
        match op {
            BrainOp::Report { text, priority } => {
                let spoken = self.speak_brain_report(text, priority, cx);
                output(json!({"queued": spoken}))
            }
            BrainOp::Approvals => output(self.approvals_json()),
            BrainOp::Decide {
                approval_id,
                decision,
                reason,
            } => self.decide_approval(&approval_id, decision, &reason, cx),
            BrainOp::Agents => output(self.agents_json()),
            BrainOp::Assign { agent, task } => self.assign_task(agent, task, window),
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

    fn assign_task(
        &mut self,
        agent: String,
        task: String,
        window: Option<&mut Window>,
    ) -> OpResult {
        let area = self.mobile_canvas_area(window);
        let zoom = self.runtime.document.viewport.zoom;
        let command = CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment { name: agent, task }],
            force_untracked: false,
        };
        crate::app::execute_high_level_agent_command(
            self,
            command,
            area,
            zoom,
            agent::AgentDispatchSource::norma(),
            None,
        )
        .map_err(|error| reject(&error.to_string()))
        .and_then(|result| output(json!({"result": format!("{result:?}")})))
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
        let high_risk = matches!(risk_class(&tracked.subject), RiskClass::High(_));
        let outcome = match decision {
            BrainDecision::Escalate => {
                self.escalate(id, reason, cx);
                return output(json!({"decision": "escalate"}));
            }
            // 大脑只能升级，不能把高风险操作降级放行。
            BrainDecision::Allow if high_risk => {
                self.escalate(id, "命中高风险规则", cx);
                return Err(reject("high_risk：命中高风险规则，已转交用户"));
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
        let summary = self
            .brain
            .tracked
            .remove(id)
            .map(|t| approvals::summarize(&t.subject));
        BrainHostState::log_decision(id, decision, reason, summary.as_deref());
        output(json!({"decision": format!("{decision:?}")}))
    }
}

impl BrainHostState {
    /// 追加一条决策记录，用户可以据此追问“为什么放行”。
    fn log_decision(id: &str, decision: BrainDecision, reason: &str, summary: Option<&str>) {
        let Some(dir) = super::config::state_dir() else {
            return;
        };
        let line = json!({"approval": id, "decision": format!("{decision:?}"), "reason": reason, "what": summary});
        let result = std::fs::create_dir_all(&dir).and_then(|_| {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("approvals.jsonl"))?;
            writeln!(file, "{line}")
        });
        if let Err(error) = result {
            tracing::warn!(%error, "写入审批记录失败");
        }
    }
}
