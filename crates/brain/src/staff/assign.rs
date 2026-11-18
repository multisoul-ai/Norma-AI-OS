//! 幕僚自己派活：宿主传入自主权开关，由策略决定直接派发还是降级为提议。
use super::policy::{decide_dispatch, AssignDecision, DispatchContext, ProposeReason};
use super::TaskStore;
use norma::protocol::{StaffOp, StaffPart, StaffProposal, Task};
use std::io;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssignOutcome {
    /// 已生成一条已批准的提议，宿主接着启动 Worker 并绑定。
    Dispatch { proposal_id: u64 },
    /// 条件不满足，已降级为等用户采用的提议。
    Proposed {
        proposal_id: u64,
        reason: ProposeReason,
    },
}

impl TaskStore {
    /// `auto` 是幕僚开关 `staff_auto_dispatch`。`named` 表示这次派活点了名册里的人。
    /// `op` 必须是 `StaffOp::Assign`。
    pub fn assign(
        &self,
        workspace: &str,
        op: StaffOp,
        auto: bool,
        named: bool,
    ) -> io::Result<(Task, AssignOutcome)> {
        let StaffOp::Assign {
            task_id,
            agent,
            title,
            prompt,
            why,
            worker,
            model,
        } = op
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "assign requires an assign operation",
            ));
        };
        let mut proposal_id = 0;
        let mut decision = AssignDecision::Dispatch;
        let task = self.update(workspace, &task_id, |task| {
            let text = format!("{title}\n{prompt}\n{why}");
            let user_turn = task.user_turn_running();
            if let Some(dispatch_id) = &worker {
                check_continuable(task, dispatch_id, &agent)?;
            }
            // 次数只记事件轮。用户轮在发话时已经清零，这次派活不再占用额度。
            let counts = !user_turn;
            decision = decide_dispatch(&DispatchContext {
                user_turn,
                named: user_turn && named,
                auto_dispatch: auto,
                auto_dispatches: task.auto_dispatches,
                text: &text,
            });
            let dispatch = decision == AssignDecision::Dispatch;
            proposal_id = task.proposals.iter().map(|p| p.id).max().unwrap_or(0) + 1;
            task.proposals.push(StaffProposal {
                id: proposal_id,
                agent,
                title,
                prompt,
                why,
                model: model.filter(|m| !m.trim().is_empty()),
                approved: dispatch,
                ignored: false,
                dispatch_id: None,
                round: task.round,
                auto: dispatch,
                downgrade: match &decision {
                    AssignDecision::Propose(reason) => Some(reason.code()),
                    AssignDecision::Dispatch => None,
                },
                superseded: false,
                target_worker: worker,
            });
            super::proposals::supersede_same_title(task, proposal_id);
            task.attach_card(StaffPart::Proposal(proposal_id));
            if dispatch && counts {
                task.auto_dispatches += 1;
            }
            task.updated_at = norma::protocol::timestamp();
            Ok(())
        })?;
        let outcome = match decision {
            AssignDecision::Dispatch => AssignOutcome::Dispatch { proposal_id },
            AssignDecision::Propose(reason) => AssignOutcome::Proposed {
                proposal_id,
                reason,
            },
        };
        Ok((task, outcome))
    }
}

/// 续派目标必须是本任务里已经结束的同一 Agent 的 Worker；执行中的不能再塞第二条 prompt。
fn check_continuable(task: &Task, dispatch_id: &str, agent: &str) -> io::Result<()> {
    let refuse = |code: &'static str| Err(io::Error::new(io::ErrorKind::InvalidInput, code));
    match task.worker_for_dispatch(dispatch_id) {
        None => refuse("staff.worker_unknown"),
        Some(worker) if !worker.is_agent(agent) => refuse("staff.worker_agent_mismatch"),
        Some(worker) if worker.outcome().is_none() => refuse("staff.worker_busy"),
        Some(_)
            if task.proposals.iter().any(|p| {
                p.target_worker.as_deref() == Some(dispatch_id)
                    && p.dispatch_id.is_none()
                    && !p.ignored
                    && !p.superseded
            }) =>
        {
            refuse("staff.worker_pending")
        }
        Some(_) => Ok(()),
    }
}
