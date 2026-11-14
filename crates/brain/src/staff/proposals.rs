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
