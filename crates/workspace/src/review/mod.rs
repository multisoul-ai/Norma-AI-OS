//! Immutable bytes for reviewing current workspace changes. This does not attribute authorship.
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path, time::Instant};
mod baseline;
mod process;
#[cfg(unix)]
mod scoped;
#[cfg(unix)]
use scoped::Root;

// Each side is bounded to1MiB; bytes plus diffs for the entire capture are bounded to8MiB.
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_REVIEW_BYTES: usize = 8 * MAX_FILE_BYTES;
const MAX_FILES: usize = 128;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Text,
    Binary,
    Symlink,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FileVersion {
    pub kind: ContentKind,
    pub executable: bool,
    pub bytes: Vec<u8>,
}
impl FileVersion {
    fn regular(bytes: Vec<u8>, executable: bool) -> Self {
        let kind = if !bytes.contains(&0) && std::str::from_utf8(&bytes).is_ok() {
            ContentKind::Text
        } else {
            ContentKind::Binary
        };
        Self {
            kind,
            executable,
            bytes,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub status: String,
    pub before: Option<FileVersion>,
    pub after: Option<FileVersion>,
    pub index: Option<FileVersion>,
    pub diff: Option<String>,
    pub staged_diff: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProjectReview {
    pub base_commit: Option<String>,
    pub files: Vec<FileChange>,
    pub conflicted: bool,
}
impl ProjectReview {
    /// Validate a locally saved baseline before it can become Git arguments or filesystem paths.
    pub fn validate(&self) -> Result<(), ReviewError> {
        if self.base_commit.as_ref().is_some_and(|hash| {
            !matches!(hash.len(), 40 | 64) || !hash.bytes().all(|b| b.is_ascii_hexdigit())
        }) {
            return Err(ReviewError::Unavailable);
        }
        if self.files.len() > MAX_FILES {
            return Err(ReviewError::TooLarge);
        }
        let mut paths = std::collections::BTreeSet::new();
        let mut size = 0;
        for file in &self.files {
            scoped::validate(&file.path)?;
            if !paths.insert(&file.path)
                || file.status.len() != 2
                || !file.status.chars().all(|c| " MADTU?!".contains(c))
            {
                return Err(ReviewError::Unavailable);
            }
            for version in [&file.before, &file.index, &file.after]
                .into_iter()
                .flatten()
            {
                if version.bytes.len() > MAX_FILE_BYTES {
                    return Err(ReviewError::TooLarge);
                }
                if (version.kind == ContentKind::Text
                    && (version.bytes.contains(&0) || std::str::from_utf8(&version.bytes).is_err()))
                    || (version.kind == ContentKind::Symlink && version.executable)
                {
                    return Err(ReviewError::Unavailable);
                }
                size += version.bytes.len();
            }
            size += file.diff.as_ref().map_or(0, String::len)
                + file.staged_diff.as_ref().map_or(0, String::len);
        }
        if size > MAX_REVIEW_BYTES {
            return Err(ReviewError::TooLarge);
        }
        if self.conflicted
            != self
                .files
                .iter()
                .any(|f| f.status.contains('U') || matches!(f.status.as_str(), "AA" | "DD"))
        {
            return Err(ReviewError::Unavailable);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewError {
    Expired,
    TooLarge,
    UnsafePath,
    Changed,
    Unavailable,
    Unsupported,
}

/// Blocking local-only read. Caller supplies the authorized root, never a remote filesystem path.
#[cfg(unix)]
pub fn capture(root: &Path, deadline: Instant) -> Result<ProjectReview, ReviewError> {
    capture_inner(root, None, deadline)
}

/// Compare against the saved task-start working copy, including commits made after that start.
/// This is still project scope; concurrent Agents' edits are not attributed to a particular author.
pub fn capture_since(
    root: &Path,
    start: &ProjectReview,
    deadline: Instant,
) -> Result<ProjectReview, ReviewError> {
    start.validate()?;
    capture_inner(root, Some(start), deadline)
}

fn capture_inner(
    root: &Path,
    start: Option<&ProjectReview>,
    deadline: Instant,
) -> Result<ProjectReview, ReviewError> {
    check_deadline(deadline)?;
    let root = Root::open(root)?;
    process::check_repository(&root, deadline)?;
    let current_head = head(&root, deadline)?;
    let base_commit = start
        .map(|s| s.base_commit.clone())
        .unwrap_or_else(|| current_head.clone());
    let prefix = git_text(&root, &["rev-parse", "--show-prefix"], deadline, 4096)?;
    let prefix = prefix.strip_suffix('\n').ok_or(ReviewError::Unavailable)?;
    let names = status(&root, prefix, deadline)?;
    let mut paths = std::collections::BTreeMap::new();
    for entry in names.split(|b| *b == 0).filter(|part| !part.is_empty()) {
        if entry.len() < 4 || entry[2] != b' ' {
            return Err(ReviewError::Unavailable);
        }
        let path = std::str::from_utf8(&entry[3..]).map_err(|_| ReviewError::Unsupported)?;
        scoped::validate(path)?;
        let state = std::str::from_utf8(&entry[..2]).map_err(|_| ReviewError::Unavailable)?;
        if state.contains(['R', 'C']) {
            return Err(ReviewError::Unavailable);
        }
        if paths.insert(path.to_owned(), state.to_owned()).is_some() {
            return Err(ReviewError::Unavailable);
        }
        if paths.len() > MAX_FILES {
            return Err(ReviewError::TooLarge);
        }
    }
    if let Some(start) = start {
        for path in start
            .files
            .iter()
            .map(|f| f.path.clone())
            .chain(baseline::committed_paths(
                &root,
                base_commit.as_deref(),
                current_head.as_deref(),
                deadline,
            )?)
        {
            paths.entry(path).or_insert_with(|| "  ".into());
        }
        if paths.len() > MAX_FILES {
            return Err(ReviewError::TooLarge);
        }
    }
    let mut files = Vec::new();
    let mut total = 0;
    for (path, state) in paths {
        check_deadline(deadline)?;
        // ponytail: one Git blob process per changed file; use cat-file --batch if128-file captures hit the deadline.
        let saved_file = start.and_then(|s| s.files.iter().find(|f| f.path == path));
        let before = if let Some(saved) = saved_file {
            saved.after.clone()
        } else if let Some(commit) = &base_commit {
            baseline(&root, commit, &path, deadline)?
        } else {
            None
        };
        let after = root.read(&path)?;
        let index = index_version(&root, &path, deadline)?;
        let initial_index = saved_file
            .map(|f| f.index.clone())
            .unwrap_or_else(|| before.clone());
        if start.is_some()
            && before == after
            && initial_index == index
            && !state.contains('U')
            && !matches!(state.as_str(), "AA" | "DD")
        {
            continue;
        }
        let diff = text_diff(before.as_ref(), after.as_ref(), deadline)?;
        let staged_diff = if state.contains('U') || matches!(state.as_str(), "AA" | "DD") {
            None
        } else {
            text_diff(before.as_ref(), index.as_ref(), deadline)?
        };
        total += before.as_ref().map_or(0, |v| v.bytes.len())
            + after.as_ref().map_or(0, |v| v.bytes.len())
            + index.as_ref().map_or(0, |v| v.bytes.len())
            + staged_diff.as_ref().map_or(0, String::len)
            + diff.as_ref().map_or(0, String::len);
        if total > MAX_REVIEW_BYTES {
            return Err(ReviewError::TooLarge);
        }
        files.push(FileChange {
            path,
            status: state,
            before,
            after,
            index,
            diff,
            staged_diff,
        });
    }
    if head(&root, deadline)? != current_head || status(&root, prefix, deadline)? != names {
        return Err(ReviewError::Changed);
    }
    let conflicted = files
        .iter()
        .any(|f| f.status.contains('U') || matches!(f.status.as_str(), "AA" | "DD"));
    Ok(ProjectReview {
        base_commit,
        files,
        conflicted,
    })
}

#[cfg(unix)]
fn head(root: &Root, deadline: Instant) -> Result<Option<String>, ReviewError> {
    let (code, bytes) = process::git(root, &["rev-parse", "--verify", "HEAD"], deadline, 128)?;
    if code == 128 {
        // Distinguish an unborn repository from a non-repository.
        git_text(root, &["rev-parse", "--git-dir"], deadline, 4096)?;
        return Ok(None);
    }
    let text = String::from_utf8(bytes).map_err(|_| ReviewError::Unavailable)?;
    let hash = text.trim();
    if code != 0 || !matches!(hash.len(), 40 | 64) || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ReviewError::Unavailable);
    }
    Ok(Some(hash.to_owned()))
}
#[cfg(unix)]
fn status(root: &Root, prefix: &str, deadline: Instant) -> Result<Vec<u8>, ReviewError> {
    let (code, bytes) = process::git(
        root,
        &[
            "-c",
            "status.relativePaths=true",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
            "--",
            ".",
        ],
        deadline,
        512 * 1024,
    )?;
    if code != 0 {
        return Err(ReviewError::Unavailable);
    }
    let mut visible = Vec::new();
    for entry in bytes.split(|b| *b == 0).filter(|part| !part.is_empty()) {
        let path = entry.get(3..).ok_or(ReviewError::Unavailable)?;
        let path = std::str::from_utf8(path).map_err(|_| ReviewError::Unsupported)?;
        // Porcelain v1 -z uses repository-relative paths even for an authorized subdirectory.
        let path = path.strip_prefix(prefix).ok_or(ReviewError::UnsafePath)?;
        if scoped::is_internal(path) {
            continue;
        }
        visible.extend_from_slice(&entry[..3]);
        visible.extend_from_slice(path.as_bytes());
        visible.push(0);
    }
    Ok(visible)
}
#[cfg(unix)]
fn git_text(
    root: &Root,
    args: &[&str],
    deadline: Instant,
    limit: usize,
) -> Result<String, ReviewError> {
    let (code, bytes) = process::git(root, args, deadline, limit)?;
    if code != 0 {
        return Err(ReviewError::Unavailable);
    }
    String::from_utf8(bytes).map_err(|_| ReviewError::Unsupported)
}
#[cfg(unix)]
fn baseline(
    root: &Root,
    commit: &str,
    path: &str,
    deadline: Instant,
) -> Result<Option<FileVersion>, ReviewError> {
    let entry = git_text(root, &["ls-tree", "-z", commit, "--", path], deadline, 8192)?;
    if entry.is_empty() {
        return Ok(None);
    }
    let metadata = entry.split_once('\t').ok_or(ReviewError::Unavailable)?.0;
    let fields = metadata.split(' ').collect::<Vec<_>>();
    if fields.len() != 3 || fields[1] != "blob" {
        return Err(ReviewError::Unsupported);
    }
    version(root, fields[0], fields[2], deadline).map(Some)
}
#[cfg(unix)]
fn index_version(
    root: &Root,
    path: &str,
    deadline: Instant,
) -> Result<Option<FileVersion>, ReviewError> {
    let entry = git_text(
        root,
        &["ls-files", "--stage", "-z", "--", path],
        deadline,
        32 * 1024,
    )?;
    if entry.is_empty() {
        return Ok(None);
    }
    let metadata = entry.split_once('\t').ok_or(ReviewError::Unavailable)?.0;
    let fields = metadata.split(' ').collect::<Vec<_>>();
    if fields.len() != 3 {
        return Err(ReviewError::Unavailable);
    }
    // Unmerged index stages have no single staged version. conflicted prevents acceptance.
    if fields[2] != "0" {
        return Ok(None);
    }
    version(root, fields[0], fields[1], deadline).map(Some)
}
#[cfg(unix)]
fn version(
    root: &Root,
    mode: &str,
    hash: &str,
    deadline: Instant,
) -> Result<FileVersion, ReviewError> {
    if !matches!(mode, "100644" | "100755" | "120000")
        || !matches!(hash.len(), 40 | 64)
        || !hash.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(ReviewError::Unsupported);
    }
    let (code, bytes) = process::git(root, &["cat-file", "blob", hash], deadline, MAX_FILE_BYTES)?;
    if code != 0 {
        return Err(ReviewError::Unavailable);
    }
    Ok(if mode == "120000" {
        FileVersion {
            kind: ContentKind::Symlink,
            executable: false,
            bytes,
        }
    } else {
        FileVersion::regular(bytes, mode == "100755")
    })
}
fn text_diff(
    before: Option<&FileVersion>,
    after: Option<&FileVersion>,
    deadline: Instant,
) -> Result<Option<String>, ReviewError> {
    if before
        .into_iter()
        .chain(after)
        .any(|v| v.kind != ContentKind::Text)
    {
        return Ok(None);
    }
    let mut left = tempfile::NamedTempFile::new().map_err(|_| ReviewError::Unavailable)?;
    let mut right = tempfile::NamedTempFile::new().map_err(|_| ReviewError::Unavailable)?;
    left.write_all(before.map_or(&[], |v| v.bytes.as_slice()))
        .map_err(|_| ReviewError::Unavailable)?;
    right
        .write_all(after.map_or(&[], |v| v.bytes.as_slice()))
        .map_err(|_| ReviewError::Unavailable)?;
    let mut command = std::process::Command::new("/usr/bin/diff");
    command
        .args(["-u", "-L", "before", "-L", "after"])
        .arg(left.path())
        .arg(right.path());
    // A full replacement can add one marker per byte-sized line: at most4MiB plus a small header.
    let (code, bytes) = process::run(command, deadline, 4 * MAX_FILE_BYTES + 4096)?;
    if !matches!(code, 0 | 1) {
        return Err(ReviewError::Unavailable);
    }
    Ok(Some(
        String::from_utf8(bytes).map_err(|_| ReviewError::Unsupported)?,
    ))
}
fn check_deadline(deadline: Instant) -> Result<(), ReviewError> {
    if Instant::now() >= deadline {
        Err(ReviewError::Expired)
    } else {
        Ok(())
    }
}
