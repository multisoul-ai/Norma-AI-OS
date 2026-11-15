//! 幕僚自己派活：宿主传入自主权开关，由策略决定直接派发还是降级为提议。
use super::policy::{decide_assign, decide_continue, AssignContext, AssignDecision, ProposeReason};
use super::TaskStore;
use norma::protocol::{StaffOp, StaffPart, StaffProposal, Task, WorkerOutcome};
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
    /// `auto` 是设置里的「后续任务」是否为「自动」。`op` 必须是 `StaffOp::Assign`。
    pub fn assign(
        &self,
        workspace: &str,
        op: StaffOp,
        auto: bool,
    ) -> io::Result<(Task, AssignOutcome)> {
        let StaffOp::Assign {
            task_id,
            agent,
            title,
            prompt,
            why,
            worker,
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
            let last_finished = task
                .workers
                .iter()
                .rev()
                .find(|worker| worker.outcome == Some(WorkerOutcome::Succeeded))
                .map(|worker| worker.agent.as_str());
            let continuing = worker.is_some();
            decision = match &worker {
                Some(dispatch_id) => {
                    check_continuable(task, dispatch_id, &agent)?;
                    decide_continue(auto, task.user_turn_running(), &text)
                }
                None => {
                    let context = AssignContext {
                        auto,
                        last_finished_agent: last_finished,
                        auto_dispatches: task.auto_dispatches,
                    };
                    decide_assign(&context, &agent, &text)
                }
            };
            let dispatch = decision == AssignDecision::Dispatch;
            proposal_id = task.proposals.iter().map(|p| p.id).max().unwrap_or(0) + 1;
            task.proposals.push(StaffProposal {
                id: proposal_id,
                agent,
                title,
                prompt,
                why,
                model: None,
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
            task.attach_card(StaffPart::Proposal(proposal_id));
            if dispatch && !continuing {
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
        Some(worker) if worker.agent != agent => refuse("staff.worker_agent_mismatch"),
        Some(worker) if worker.outcome.is_none() => refuse("staff.worker_busy"),
        Some(_) => Ok(()),
    }
}
