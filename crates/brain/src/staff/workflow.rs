use super::{missing, TaskStore};
use norma::protocol::{StaffOp, StaffProposal, Task, TaskMessage, TaskPhase, Worker};
use std::io;

impl TaskStore {
    /// CLI 操作不包含用户批准入口；批准仅从真实用户动作进入 approve。
    pub fn execute(&self, workspace: &str, op: StaffOp) -> io::Result<Vec<Task>> {
        if matches!(op, StaffOp::List) {
            return self.list(workspace);
        }
        if let StaffOp::Create { title } = op {
            let id = format!(
                "task-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            return self
                .create(Task::new(&id, workspace, workspace, &title))
                .map(|task| vec![task]);
        }
        let id = op.task_id().ok_or_else(missing)?.to_owned();
        self.update(workspace, &id, |task| {
            match op {
                StaffOp::Message { text, .. } => {
                    require_text(&text)?;
                    task.user_message(&text);
                }
                StaffOp::Reply { text, .. } => {
                    require_text(&text)?;
                    task.messages.push(TaskMessage::Norma { text });
                }
                StaffOp::Phase { phase, .. } => task.set_phase(phase, false),
                StaffOp::Propose {
                    agent,
                    title,
                    prompt,
                    why,
                    ..
                } => {
                    require_text(&agent)?;
                    require_text(&prompt)?;
                    norma::protocol::validate_proposal_copy(&title, &why).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "invalid proposal copy")
                    })?;
                    // 模型重复提交同一提议时保留原编号与用户决策。
                    // 需要你的 Worker 等用户在原卡重新投递，不再为同一 agent 与 prompt 新开一张卡。
                    let duplicate = task.proposals.iter().any(|proposal| {
                        proposal.agent == agent
                            && proposal.prompt == prompt
                            && proposal.dispatch_id.is_none()
                    });
                    let awaiting = task.workers.iter().any(|worker| {
                        worker.state == norma::protocol::WorkerState::NeedsAttention
                            && worker.agent == agent
                            && task.proposals.iter().any(|proposal| {
                                proposal.agent == agent
                                    && proposal.prompt == prompt
                                    && proposal.dispatch_id.as_deref()
                                        == Some(worker.dispatch_id.as_str())
                            })
                    });
                    if duplicate || awaiting {
                        return Ok(());
                    }
                    let id = task.proposals.len() as u64 + 1;
                    task.proposals.push(StaffProposal {
                        id,
                        agent,
                        title,
                        prompt,
                        why,
                        approved: false,
                        dispatch_id: None,
                    });
                    task.messages
                        .push(TaskMessage::Proposal { proposal_id: id });
                }
                StaffOp::Adopt { .. } => {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "adoption requires host dispatch",
                    ))
                }
                StaffOp::Create { .. } | StaffOp::List => {
                    unreachable!("handled before task lookup")
                }
            }
            task.updated_at = norma::protocol::timestamp();
            Ok(())
        })
        .map(|task| vec![task])
    }

    pub fn approve(&self, workspace: &str, id: &str, proposal_id: u64) -> io::Result<Task> {
        self.update(workspace, id, |task| {
            let proposal = task
                .proposals
                .iter_mut()
                .find(|p| p.id == proposal_id)
                .ok_or_else(missing)?;
            proposal.approved = true;
            Ok(())
        })
    }

    pub fn bind_worker(
        &self,
        workspace: &str,
        id: &str,
        proposal_id: u64,
        worker: Worker,
    ) -> io::Result<Task> {
        self.transact(workspace, true, |tasks| {
            if tasks.iter().any(|task| {
                task.workers.iter().any(|old| {
                    old.card_id == worker.card_id || old.dispatch_id == worker.dispatch_id
                })
            }) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "worker already bound",
                ));
            }
            let task = tasks
                .iter_mut()
                .find(|task| task.id == id)
                .ok_or_else(missing)?;
            let proposal = task
                .proposals
                .iter_mut()
                .find(|p| p.id == proposal_id)
                .ok_or_else(missing)?;
            if !proposal.approved
                || proposal.dispatch_id.is_some()
                || proposal.agent != worker.agent
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "proposal not approved or already dispatched",
                ));
            }
            require_text(&worker.dispatch_id)?;
            if worker.card_id.is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "worker requires card",
                ));
            }
            proposal.dispatch_id = Some(worker.dispatch_id.clone());
            task.messages.push(TaskMessage::WorkerStarted {
                proposal_id,
                dispatch_id: worker.dispatch_id.clone(),
            });
            task.workers.push(worker);
            task.revision = task
                .revision
                .checked_add(1)
                .ok_or_else(|| io::Error::other("task revision overflow"))?;
            task.set_phase(TaskPhase::Active, false);
            task.updated_at = norma::protocol::timestamp();
            Ok(task.clone())
        })
    }

    pub fn summary_for(&self, workspace: &str) -> io::Result<Option<String>> {
        let tasks = self.list(workspace)?;
        Ok((!tasks.is_empty()).then(|| tasks.iter().map(|task| {
            let attention: Vec<_> = task.workers.iter().filter(|w| w.needs_input || w.state != norma::protocol::WorkerState::Active
                || w.outcome == Some(norma::protocol::WorkerOutcome::Unavailable)).map(|w| &w.agent).collect();
            serde_json::json!({"task":task.id,"title":task.title,"phase":task.phase,"status":task.display_status(),"needs_you":attention}).to_string()
        }).collect::<Vec<_>>().join("\n")))
    }
}

fn require_text(text: &str) -> io::Result<()> {
    if text.trim().is_empty() {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "empty staff message",
        ))
    } else {
        Ok(())
    }
}

impl TaskStore {
    pub(super) fn record_attention(
        &self,
        workspace: &str,
        node: u64,
        dispatch: &str,
        waiting: bool,
    ) -> io::Result<super::TaskEventResult> {
        use super::TaskEventResult;
        let tasks = self.list(workspace)?;
        let Some(task) = tasks.iter().find(|task| {
            task.workers
                .iter()
                .any(|worker| worker.card_id == Some(node))
        }) else {
            return Ok(TaskEventResult::Unowned);
        };
        let Some(worker) = task
            .workers
            .iter()
            .find(|worker| worker.card_id == Some(node) && worker.dispatch_id == dispatch)
        else {
            return Ok(TaskEventResult::Ignored);
        };
        if worker.outcome.is_some() || worker.needs_input == waiting {
            return Ok(TaskEventResult::Ignored);
        }
        self.update(workspace, &task.id, |task| {
            if let Some(worker) = task.workers.iter_mut().find(|worker| {
                worker.card_id == Some(node)
                    && worker.dispatch_id == dispatch
                    && worker.outcome.is_none()
            }) {
                worker.needs_input = waiting;
            }
            Ok(())
        })?;
        Ok(TaskEventResult::Recorded {
            task_id: task.id.clone(),
            wake: false,
        })
    }
}
