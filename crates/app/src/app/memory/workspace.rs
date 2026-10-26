use agent::{AdapterError, MemoryRevision, MemoryScope, MemoryScopeAlias};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// Version metadata only. Project bodies remain behind authorized workspace_memory calls.
pub(super) struct WorkspaceMemory {
    paths: storage::DatabasePaths,
    roots: Arc<Mutex<Vec<PathBuf>>>,
    accesses: BTreeMap<PathBuf, storage::MemoryAccess>,
}

impl WorkspaceMemory {
    pub(super) fn new(paths: storage::DatabasePaths, roots: Arc<Mutex<Vec<PathBuf>>>) -> Self {
        Self {
            paths,
            roots,
            accesses: BTreeMap::new(),
        }
    }

    /// Called only inside MemoryAdapter's blocking background task.
    pub(super) fn revisions(&mut self) -> Result<Vec<(MemoryScope, MemoryRevision)>, AdapterError> {
        let mut roots = self
            .roots
            .lock()
            .map_err(|_| AdapterError::Failed("工作空间记忆范围不可用".into()))?
            .clone();
        roots.sort();
        roots.dedup();
        self.accesses.retain(|root, _| roots.contains(root));
        let mut revisions = Vec::new();
        for root in roots {
            let access = self
                .accesses
                .entry(root.clone())
                .or_insert_with(|| storage::MemoryAccess::new(self.paths.clone(), Some(root)));
            for alias in [MemoryScopeAlias::Project, MemoryScopeAlias::Worktree] {
                let revision = access.scope_revision(alias)?;
                if !revisions.iter().any(|(scope, _)| scope == &revision.0) {
                    revisions.push(revision);
                }
            }
        }
        Ok(revisions)
    }
}
