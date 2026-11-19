use super::policy::normalize;
use super::{missing, TaskStore};
use norma::protocol::Task;
use std::io;

impl TaskStore {
    /// Ignores exactly one proposal without waking the agent or affecting its newer proposals.
    pub fn set_proposal_ignored(
        &self,
        workspace: &str,
        id: &str,
        proposal_id: u64,
        ignored: bool,
    ) -> io::Result<Task> {
        self.update(workspace, id, |task| {
            if !ignored
                && task
                    .proposals
                    .iter()
                    .any(|proposal| proposal.id > proposal_id)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "staff.proposal_superseded",
                ));
            }
            let proposal = task
                .proposals
                .iter_mut()
                .find(|proposal| proposal.id == proposal_id)
                .ok_or_else(missing)?;
            if proposal.approved || proposal.dispatch_id.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "staff.proposal_unavailable",
                ));
            }
            proposal.ignored = ignored;
            task.updated_at = norma::protocol::timestamp();
            Ok(())
        })
    }
}

/// 刚入库的提议（`keep`）之外，同一 agent、标题归一化后相同、仍待采用的旧提议标记为已替换。
/// 同一回合内措辞不同的重复提议也在此收敛；已采用、已忽略、已派发的不动。
pub(super) fn supersede_same_title(task: &mut Task, keep: u64) {
    let Some((agent, title)) = task
        .proposals
        .iter()
        .find(|p| p.id == keep)
        .map(|p| (p.agent.clone(), normalize(&p.title)))
    else {
        return;
    };
    for proposal in &mut task.proposals {
        if proposal.id != keep
            && proposal.agent == agent
            && !proposal.approved
            && !proposal.ignored
            && proposal.dispatch_id.is_none()
            && normalize(&proposal.title) == title
        {
            proposal.superseded = true;
        }
    }
}
