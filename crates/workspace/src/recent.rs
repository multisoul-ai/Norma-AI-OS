use crate::display_name_for_path;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_RECENT_WORKSPACES: usize = 50;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkspaceSource {
    Open,
    Clone,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceRecord {
    pub path: PathBuf,
    pub display_name: String,
    pub last_opened_at_unix: u64,
    pub source: WorkspaceSource,
}

impl WorkspaceRecord {
    pub fn new(path: PathBuf, last_opened_at_unix: u64, source: WorkspaceSource) -> Self {
        let display_name = display_name_for_path(&path);
        WorkspaceRecord {
            path,
            display_name,
            last_opened_at_unix,
            source,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecentWorkspaces {
    pub records: Vec<WorkspaceRecord>,
}

impl RecentWorkspaces {
    pub fn record_opened_at(
        &mut self,
        path: PathBuf,
        source: WorkspaceSource,
        last_opened_at_unix: u64,
    ) -> WorkspaceRecord {
        let record = WorkspaceRecord::new(path, last_opened_at_unix, source);
        self.records.retain(|existing| existing.path != record.path);
        self.records.push(record.clone());
        self.sort_and_truncate();
        record
    }

    pub fn top(&self, limit: usize) -> &[WorkspaceRecord] {
        let end = self.records.len().min(limit);
        &self.records[..end]
    }

    fn sort_and_truncate(&mut self) {
        self.records.sort_by(|left, right| {
            right
                .last_opened_at_unix
                .cmp(&left.last_opened_at_unix)
                .then_with(|| left.display_name.cmp(&right.display_name))
                .then_with(|| left.path.cmp(&right.path))
        });
        self.records.truncate(MAX_RECENT_WORKSPACES);
    }
}

pub fn canonical_workspace_path(path: &Path) -> std::io::Result<PathBuf> {
    path.canonicalize()
}

#[cfg(test)]
#[path = "../tests/unit/recent_tests.rs"]
mod tests;
