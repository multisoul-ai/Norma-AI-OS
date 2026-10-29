use super::ContentError;
use super::{files::fingerprint, Project, Result};
use git2::{DiffOptions, Repository, Status, StatusOptions};
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Comparison {
    #[default]
    Local,
    Branch(String),
}

#[derive(Clone, Debug)]
pub struct Change {
    pub path: String,
    pub old_path: Option<String>,
    pub status: ChangeStatus,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
    pub conflicted: bool,
}

#[derive(Clone, Debug)]
pub struct GitSnapshot {
    pub branch: Option<String>,
    pub version: String,
    pub branches: Vec<String>,
    pub default_target: Option<String>,
    pub changes: Vec<Change>,
    pub writable: bool,
}

fn head_tree(repo: &Repository) -> Option<git2::Tree<'_>> {
    repo.head().ok()?.peel_to_tree().ok()
}

fn base_tree<'a>(repo: &'a Repository, target: &str) -> Result<git2::Tree<'a>> {
    if target.starts_with('-') {
        return Err(ContentError::InvalidBranch);
    }
    let target = repo
        .revparse_single(target)
        .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?
        .peel_to_commit()
        .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?;
    let head = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?;
    let base = repo
        .merge_base(head.id(), target.id())
        .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?;
    repo.find_commit(base)
        .and_then(|c| c.tree())
        .map_err(|e| ContentError::Diagnostic(e.message().to_string()))
}

impl Project {
    pub fn git_snapshot(&self, comparison: &Comparison) -> Result<GitSnapshot> {
        let repo = self.repository()?;
        let mut options = StatusOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .renames_head_to_index(true)
            .renames_index_to_workdir(false);
        let statuses = repo
            .statuses(Some(&mut options))
            .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?;
        let mut version = Vec::new();
        version.extend_from_slice(
            repo.head()
                .ok()
                .and_then(|h| h.target())
                .map(|id| id.to_string())
                .unwrap_or_default()
                .as_bytes(),
        );
        version.extend_from_slice(&std::fs::read(repo.path().join("index")).unwrap_or_default());
        let mut changes = Vec::new();
        for entry in statuses.iter() {
            let renamed = entry
                .head_to_index()
                .filter(|d| d.status() == git2::Delta::Renamed);
            let path = renamed
                .as_ref()
                .and_then(|d| d.new_file().path())
                .and_then(Path::to_str)
                .or_else(|| entry.path())
                .ok_or(ContentError::InvalidFilename)?
                .to_string();
            let status = entry.status();
            version.extend_from_slice(path.as_bytes());
            version.extend_from_slice(&status.bits().to_le_bytes());
            let full = self.root.join(&path);
            let metadata = std::fs::symlink_metadata(&full).ok();
            if metadata
                .as_ref()
                .is_some_and(|m| m.file_type().is_symlink())
            {
                version.extend_from_slice(
                    std::fs::read_link(&full)
                        .map_err(|e| e.to_string())?
                        .as_os_str()
                        .as_encoded_bytes(),
                );
            } else if metadata.is_some_and(|m| m.is_file()) {
                // Hash on the worker, without buffering large binaries in memory.
                let oid = git2::Oid::hash_file(git2::ObjectType::Blob, &full)
                    .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?;
                version.extend_from_slice(oid.as_bytes());
            }
            changes.push(Change {
                old_path: renamed
                    .as_ref()
                    .and_then(|d| d.old_file().path())
                    .and_then(Path::to_str)
                    .filter(|old| *old != path)
                    .map(str::to_string),
                path,
                status: if status.is_conflicted() {
                    ChangeStatus::Conflicted
                } else if status.contains(Status::WT_NEW) {
                    ChangeStatus::Added
                } else if status.intersects(Status::WT_DELETED | Status::INDEX_DELETED) {
                    ChangeStatus::Deleted
                } else if status.intersects(Status::WT_RENAMED | Status::INDEX_RENAMED) {
                    ChangeStatus::Renamed
                } else {
                    ChangeStatus::Modified
                },
                staged: status.intersects(
                    Status::INDEX_NEW
                        | Status::INDEX_DELETED
                        | Status::INDEX_MODIFIED
                        | Status::INDEX_RENAMED
                        | Status::INDEX_TYPECHANGE,
                ),
                unstaged: status.intersects(
                    Status::WT_MODIFIED
                        | Status::WT_DELETED
                        | Status::WT_RENAMED
                        | Status::WT_TYPECHANGE
                        | Status::WT_NEW,
                ),
                untracked: status.contains(Status::WT_NEW),
                conflicted: status.is_conflicted(),
            });
        }
        let writable =
            repo.state() == git2::RepositoryState::Clean && !changes.iter().any(|c| c.conflicted);
        version.extend_from_slice(format!("{:?}", repo.state()).as_bytes());
        let branches = repo
            .branches(None)
            .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?
            .filter_map(|b| b.ok())
            .filter_map(|(b, _)| b.name().ok().flatten().map(str::to_string))
            .filter(|b| !b.ends_with("/HEAD"))
            .collect::<Vec<_>>();
        let default_target = repo
            .find_reference("refs/remotes/origin/HEAD")
            .ok()
            .and_then(|r| {
                r.symbolic_target()
                    .map(|s| s.trim_start_matches("refs/remotes/").to_string())
            })
            .or_else(|| {
                ["main", "master"]
                    .iter()
                    .find(|name| branches.iter().any(|b| b == **name))
                    .map(|s| s.to_string())
            });
        if let Comparison::Branch(target) = comparison {
            let base = base_tree(&repo, target)?;
            version.extend_from_slice(base.id().as_bytes());
            version.extend_from_slice(
                repo.revparse_single(target)
                    .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?
                    .id()
                    .as_bytes(),
            );
            let head = head_tree(&repo);
            let mut diff = repo
                .diff_tree_to_tree(Some(&base), head.as_ref(), None)
                .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?;
            diff.find_similar(None)
                .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?;
            changes = diff
                .deltas()
                .map(|d| Change {
                    path: d
                        .new_file()
                        .path()
                        .or(d.old_file().path())
                        .unwrap_or(Path::new(""))
                        .to_string_lossy()
                        .to_string(),
                    old_path: d.old_file().path().map(|p| p.to_string_lossy().to_string()),
                    status: ChangeStatus::from(d.status()),
                    staged: false,
                    unstaged: false,
                    untracked: false,
                    conflicted: false,
                })
                .collect();
        }
        Ok(GitSnapshot {
            branch: repo
                .head()
                .ok()
                .and_then(|h| h.shorthand().map(str::to_string)),
            version: fingerprint(&version),
            branches,
            default_target,
            changes,
            writable,
        })
    }

    pub fn git_diff(&self, comparison: &Comparison, path: &str, staged: bool) -> Result<String> {
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(ContentError::InvalidPath);
        }
        let repo = self.repository()?;
        let head = head_tree(&repo);
        let mut options = DiffOptions::new();
        options
            .pathspec(path)
            .disable_pathspec_match(true)
            .include_untracked(true)
            .show_untracked_content(true)
            .recurse_untracked_dirs(true)
            .max_size(super::MAX_TEXT_BYTES as i64);
        if let Some(old) = self
            .git_snapshot(comparison)?
            .changes
            .iter()
            .find(|c| c.path == path)
            .and_then(|c| c.old_path.as_ref())
        {
            options.pathspec(old);
        }
        let mut diff = match comparison {
            Comparison::Branch(target) => repo.diff_tree_to_tree(
                Some(&base_tree(&repo, target)?),
                head.as_ref(),
                Some(&mut options),
            ),
            Comparison::Local if staged => {
                repo.diff_tree_to_index(head.as_ref(), None, Some(&mut options))
            }
            Comparison::Local => repo.diff_index_to_workdir(None, Some(&mut options)),
        }
        .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?;
        diff.find_similar(None)
            .map_err(|e| ContentError::Diagnostic(e.message().to_string()))?;
        let mut text = Vec::new();
        let mut large = false;
        diff.print(git2::DiffFormat::Patch, |_, _, line| {
            if text.len() + line.content().len() > super::MAX_TEXT_BYTES as usize {
                large = true;
                return false;
            }
            if matches!(line.origin(), '+' | '-' | ' ') {
                text.push(line.origin() as u8);
            }
            text.extend_from_slice(line.content());
            true
        })
        .map_err(|e| {
            if large {
                ContentError::DiffTooLarge
            } else {
                ContentError::Diagnostic(e.message().to_string())
            }
        })?;
        String::from_utf8(text).map_err(|_| ContentError::BinaryDiff)
    }
}

/// File-change semantics shared by local and branch comparisons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeStatus {
    Unchanged,
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
    Ignored,
    TypeChanged,
    Unreadable,
    Conflicted,
}

impl From<git2::Delta> for ChangeStatus {
    fn from(value: git2::Delta) -> Self {
        match value {
            git2::Delta::Unmodified => Self::Unchanged,
            git2::Delta::Added => Self::Added,
            git2::Delta::Untracked => Self::Added,
            git2::Delta::Deleted => Self::Deleted,
            git2::Delta::Modified => Self::Modified,
            git2::Delta::Renamed => Self::Renamed,
            git2::Delta::Copied => Self::Copied,
            git2::Delta::Ignored => Self::Ignored,
            git2::Delta::Typechange => Self::TypeChanged,
            git2::Delta::Unreadable => Self::Unreadable,
            git2::Delta::Conflicted => Self::Conflicted,
        }
    }
}
