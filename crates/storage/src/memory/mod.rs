//! Host-managed Markdown memory. These blocking APIs belong on background
//! executors; this crate does not connect them to conversation or GPUI paths.

mod document;
mod fs;
mod revision;

use crate::DatabasePaths;
use agent::{
    MemoryCommitReceipt, MemoryEntry, MemoryEntryId, MemoryMutation, MemoryRevision, MemoryScope,
    MemorySummary,
};
pub use document::{MAX_CATALOG_BYTES, MAX_TOPIC_BYTES};
use fs::Directory;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type MemoryResult<T> = Result<T, MemoryError>;

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("invalid memory path or non-regular managed file")]
    InvalidPath,
    #[error("memory scope identity must be nonempty host-provided text")]
    InvalidScope,
    #[error("invalid memory document: {0}")]
    InvalidDocument(String),
    #[error("memory byte budget exceeded")]
    TooLarge,
    #[error("memory scope has another active writer")]
    Busy,
    #[error("memory revision conflict: expected {expected:?}, current {current:?}")]
    Conflict {
        expected: MemoryRevision,
        current: MemoryRevision,
    },
    #[error("memory revision integrity check failed")]
    CorruptRevision,
    #[error("empty candidate cannot replace an existing memory collection")]
    EmptyCandidate,
    #[error("duplicate mutation or missing deletion target")]
    InvalidMutation,
    #[error("snapshot belongs to a different memory store")]
    ForeignSnapshot,
    #[error("revision counter exhausted")]
    RevisionExhausted,
    #[error("requested revision was never published in this scope")]
    RevisionNotFound,
    #[error("revision {revision:?} is visible but directory sync failed; inspect the current snapshot before retrying: {source}")]
    PublishedButUnsynced {
        revision: MemoryRevision,
        source: std::io::Error,
    },
    #[error("memory IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("memory manifest JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct MemorySnapshot {
    root: PathBuf,
    scope: MemoryScope,
    revision: MemoryRevision,
    manifest_sha256: Option<String>,
    catalog: String,
    entries: BTreeMap<MemoryEntryId, MemoryEntry>,
}

impl MemorySnapshot {
    pub fn scope(&self) -> &MemoryScope {
        &self.scope
    }
    pub fn revision(&self) -> MemoryRevision {
        self.revision
    }
    pub fn catalog(&self) -> &str {
        &self.catalog
    }
    pub fn entries(&self) -> impl Iterator<Item = &MemorySummary> {
        self.entries.values().map(|entry| &entry.summary)
    }
}

#[derive(Clone)]
pub struct MemoryStore {
    root: PathBuf,
    directory: Arc<Directory>,
}

impl MemoryStore {
    pub fn open(paths: &DatabasePaths) -> MemoryResult<Self> {
        let app_root = paths
            .app_db_path()
            .parent()
            .filter(|p| p.is_absolute())
            .ok_or(MemoryError::InvalidPath)?;
        std::fs::create_dir_all(app_root)?;
        let app = Directory::open(app_root)?;
        let norma = app.child("norma", true)?;
        let directory = norma.child("memory", true)?;
        let root = app_root.canonicalize()?.join("norma/memory");
        Ok(Self {
            root,
            directory: Arc::new(directory),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn snapshot(&self, scope: &MemoryScope) -> MemoryResult<MemorySnapshot> {
        self.load_current(scope, &self.scope_directory(scope)?)
    }

    pub fn read(
        &self,
        snapshot: &MemorySnapshot,
        id: &MemoryEntryId,
    ) -> MemoryResult<Option<MemoryEntry>> {
        if snapshot.root != self.root {
            return Err(MemoryError::ForeignSnapshot);
        }
        Ok(snapshot.entries.get(id).cloned())
    }

    pub fn commit(
        &self,
        scope: &MemoryScope,
        expected: MemoryRevision,
        changes: &[MemoryMutation],
    ) -> MemoryResult<MemoryCommitReceipt> {
        let directory = self.scope_directory(scope)?;
        let _lock = directory.lock()?;
        let current = self.load_current(scope, &directory)?;
        if current.revision != expected {
            return Err(MemoryError::Conflict {
                expected,
                current: current.revision,
            });
        }
        let mut entries = current.entries.clone();
        let mut changed = BTreeSet::new();
        for change in changes {
            let id = match change {
                MemoryMutation::Upsert { id, .. } | MemoryMutation::Delete { id } => id,
            };
            if !changed.insert(id.clone()) {
                return Err(MemoryError::InvalidMutation);
            }
            match change {
                MemoryMutation::Upsert {
                    id,
                    markdown,
                    sources,
                } => {
                    entries.insert(
                        id.clone(),
                        document::document(id.clone(), markdown.clone(), sources.clone())?,
                    );
                }
                MemoryMutation::Delete { id } => {
                    if entries.remove(id).is_none() {
                        return Err(MemoryError::InvalidMutation);
                    }
                }
            }
        }
        changed.retain(|id| entries.get(id) != current.entries.get(id));
        if changed.is_empty() {
            return Ok(MemoryCommitReceipt {
                revision: current.revision,
                changed_entries: vec![],
            });
        }
        let revision = self.publish(scope, &directory, &current, entries)?;
        Ok(MemoryCommitReceipt {
            revision,
            changed_entries: changed.into_iter().collect(),
        })
    }

    /// Restore only a revision reachable through the published ancestry. Prepared
    /// orphan directories from failed/crashed commits are never rollback targets.
    pub fn restore(
        &self,
        scope: &MemoryScope,
        expected: MemoryRevision,
        target: MemoryRevision,
    ) -> MemoryResult<MemoryCommitReceipt> {
        let directory = self.scope_directory(scope)?;
        let _lock = directory.lock()?;
        let current = self.load_current(scope, &directory)?;
        if current.revision != expected {
            return Err(MemoryError::Conflict {
                expected,
                current: current.revision,
            });
        }
        let restored = self.published_revision(scope, &directory, &current, target)?;
        let changed_entries = current
            .entries
            .keys()
            .chain(restored.entries.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|id| current.entries.get(id) != restored.entries.get(id))
            .collect();
        let revision = self.publish(scope, &directory, &current, restored.entries)?;
        Ok(MemoryCommitReceipt {
            revision,
            changed_entries,
        })
    }

    fn scope_directory(&self, scope: &MemoryScope) -> MemoryResult<Directory> {
        let project_directory = |id: &str| -> MemoryResult<Directory> {
            valid_identity(id)?;
            self.directory
                .child("projects", true)?
                .child(&revision::digest(id.as_bytes()), true)
        };
        match scope {
            MemoryScope::Personal => self.directory.child("personal", true),
            MemoryScope::ProjectShared { project_id } => {
                project_directory(project_id)?.child("shared", true)
            }
            MemoryScope::Worktree {
                project_id,
                worktree_id,
            } => {
                valid_identity(worktree_id)?;
                project_directory(project_id)?
                    .child("worktrees", true)?
                    .child(&revision::digest(worktree_id.as_bytes()), true)
            }
        }
    }
}

fn valid_identity(id: &str) -> MemoryResult<()> {
    if id.trim().is_empty() || id.contains('\0') {
        return Err(MemoryError::InvalidScope);
    }
    Ok(())
}
