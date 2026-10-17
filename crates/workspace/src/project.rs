use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectIdentity {
    /// Git common dir for repositories, canonical directory for plain projects.
    pub key: PathBuf,
    /// The chosen worktree or plain project directory.
    pub root: PathBuf,
}

pub fn identify_project(path: &Path) -> io::Result<ProjectIdentity> {
    let selected = path.canonicalize()?;
    // Match Git's filesystem and ceiling-directory discovery rules without
    // requiring the system Git executable at runtime.
    // git2 always passes a non-null ceiling list to libgit2, so forward the
    // environment value explicitly when FROM_ENV is used.
    let ceiling_dirs = std::env::var_os("GIT_CEILING_DIRECTORIES")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let repository = match git2::Repository::open_ext(
        &selected,
        git2::RepositoryOpenFlags::FROM_ENV,
        &ceiling_dirs,
    ) {
        Ok(repository) => repository,
        // A directory with broken or unreadable Git metadata is still a
        // usable plain project, as it was when `git rev-parse` failed.
        Err(_) => {
            return Ok(ProjectIdentity {
                key: selected.clone(),
                root: selected,
            });
        }
    };
    let Some(root) = repository.workdir() else {
        return Ok(ProjectIdentity {
            key: selected.clone(),
            root: selected,
        });
    };
    Ok(ProjectIdentity {
        key: repository.commondir().canonicalize()?,
        root: root.canonicalize()?,
    })
}

pub fn project_worktrees(project: &ProjectIdentity) -> io::Result<Vec<PathBuf>> {
    if project.key == project.root {
        return Ok(vec![project.root.clone()]);
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&project.root)
        .args(["worktree", "list", "--porcelain"])
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("could not list Git worktrees"));
    }
    let mut roots = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some(path) = line.strip_prefix("worktree ") else {
            continue;
        };
        let Ok(root) = PathBuf::from(path).canonicalize() else {
            continue;
        };
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    if !roots.contains(&project.root) {
        roots.push(project.root.clone());
    }
    Ok(roots)
}
