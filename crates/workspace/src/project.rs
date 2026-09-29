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
    let output = Command::new("git")
        .arg("-C")
        .arg(&selected)
        .args([
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
            "--show-toplevel",
        ])
        .stderr(Stdio::null())
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            let paths = String::from_utf8_lossy(&output.stdout);
            let mut lines = paths.lines();
            if let (Some(common), Some(root)) = (lines.next(), lines.next()) {
                return Ok(ProjectIdentity {
                    key: PathBuf::from(common).canonicalize()?,
                    root: PathBuf::from(root).canonicalize()?,
                });
            }
        }
    }
    Ok(ProjectIdentity {
        key: selected.clone(),
        root: selected,
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
