//! 幕僚自己派活：宿主传入自主权开关，由策略决定直接派发还是降级为提议。
use super::policy::{decide_dispatch, AssignDecision, DispatchContext, ProposeReason};
use super::TaskStore;
use norma::protocol::{StaffOp, StaffPart, StaffProposal, Task, WorkerOutcome, WorkerPhase};
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
    /// `auto` 是幕僚开关；`named` 表示用户点了名册里的人，原负责人另行核对收件记录。
    /// `op` 必须是 `StaffOp::Assign`。
    pub fn assign(
        &self,
        workspace: &str,
        op: StaffOp,
        auto: bool,
        named: bool,
    ) -> io::Result<(Task, AssignOutcome)> {
        self.assign_for_recipient(workspace, op, auto, named, None)
    }

    pub fn assign_for_recipient(
        &self,
        workspace: &str,
        op: StaffOp,
        auto: bool,
        named: bool,
        recipient: Option<(i64, &str)>,
    ) -> io::Result<(Task, AssignOutcome)> {
        let StaffOp::Assign {
            task_id,
            agent,
            title,
            prompt,
            why,
            owner_of,
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
            if let Some(title) = owner_of.as_deref() {
                verify_original_owner(task, title, &agent, recipient)?;
                if !user_turn || !original_owner_requested(&task.user_words) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "staff.owner_assignment_not_requested",
                    ));
                }
            }
            if let Some(dispatch_id) = &worker {
                check_continuable(task, dispatch_id, &agent)?;
            }
            // 次数只记事件轮。用户轮在发话时已经清零，这次派活不再占用额度。
            let counts = !user_turn;
            decision = decide_dispatch(&DispatchContext {
                user_turn,
                named: user_turn && (named || owner_of.is_some()),
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

/// 只允许明确的当前派发请求走原负责人直派。模型提供的正文不参与授权判断。
fn original_owner_requested(words: &str) -> bool {
    let words = super::policy::normalize(words);
    let contains = |phrases: &[&str]| phrases.iter().any(|phrase| words.contains(phrase));
    let direct = [
        "让原负责人",
        "请原负责人",
        "你应该指派",
        "请指派",
        "指派",
        "派给",
        "请派给",
        "交给",
        "请交给",
        "assign ",
        "please assign ",
        "dispatch ",
        "please dispatch ",
        "hand off ",
        "have the original owner ",
        "have the previous owner ",
        "ask the original owner ",
        "ask the previous owner ",
    ]
    .iter()
    .any(|prefix| words.starts_with(prefix));
    let refers_to_owner =
        (words.starts_with('让') || words.starts_with("请让")) && words.contains("原负责人");
    (direct || refers_to_owner || contains(&["交给原负责人", "派给原负责人"]))
        && !contains(&[
            "先给我看",
            "先确认",
            "先问我",
            "不要派",
            "别派",
            "先不派",
            "能不能",
            "是否能",
            "派给谁",
            "交给谁",
            "指派了谁",
            "进度怎么样",
            "show me first",
            "confirm first",
            "don't dispatch",
            "do not dispatch",
            "before dispatching",
        ])
}

fn verify_original_owner(
    task: &Task,
    title: &str,
    agent: &str,
    recipient: Option<(i64, &str)>,
) -> io::Result<()> {
    let mut owners = task
        .proposals
        .iter()
        .filter(|proposal| proposal.title == title)
        .filter_map(|proposal| {
            let dispatch = proposal.dispatch_id.as_deref()?;
            task.workers
                .iter()
                .chain(task.removed_workers.iter().map(|removed| &removed.worker))
                .find(|worker| worker.dispatch_id == dispatch)
                .filter(|worker| {
                    task.received_proposals.contains(&proposal.id)
                        || (task.latest_proposal_for_dispatch(dispatch) == Some(proposal.id)
                            && matches!(
                                worker.phase,
                                WorkerPhase::Running
                                    | WorkerPhase::Waiting
                                    | WorkerPhase::Finished(WorkerOutcome::Succeeded)
                            ))
                })
        });
    let matches_recipient = |worker: &norma::protocol::Worker| {
        worker.agent.eq_ignore_ascii_case(agent)
            && recipient.is_none_or(|(id, engine)| {
                worker.roster_id.map_or_else(
                    || worker.engine.eq_ignore_ascii_case(engine),
                    |owner| owner == id,
                )
            })
    };
    if !owners.next().is_some_and(matches_recipient)
        || owners.any(|owner| !matches_recipient(owner))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "staff.owner_unresolved",
        ));
    }
    Ok(())
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
