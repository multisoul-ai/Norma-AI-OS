pub(super) static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use super::{Comparison, Project, Result};
use std::{
    io::Write,
    path::{Component, Path},
};

#[derive(Clone, Debug)]
pub enum GitAction {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    Discard(Vec<String>),
    Commit(String),
}

impl Project {
    pub fn git_action(&self, expected: &str, action: GitAction) -> Result<String> {
        let _lock = WRITE_LOCK.lock().map_err(|e| e.to_string())?;
        let status = self.git_snapshot(&Comparison::Local)?;
        if status.version != expected {
            return Err("仓库已改变，请刷新后重试".into());
        }
        if !status.writable {
            return Err("仓库存在冲突或正在合并/rebase，请先在终端完成操作".into());
        }
        let paths = match &action {
            GitAction::Stage(p) | GitAction::Unstage(p) | GitAction::Discard(p) => p,
            GitAction::Commit(message) => {
                if message.trim().is_empty() {
                    return Err("请输入提交说明".into());
                }
                if !status.changes.iter().any(|c| c.staged) {
                    return Err("没有已暂存文件".into());
                }
                let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
                file.write_all(message.as_bytes())
                    .map_err(|e| e.to_string())?;
                return super::process::git(
                    &self.root,
                    &[
                        "commit",
                        "-F",
                        file.path().to_str().ok_or("提交说明路径无效")?,
                    ],
                );
            }
        };
        if paths.is_empty() {
            return Err("请选择文件".into());
        }
        let mut operation_paths = paths.clone();
        for path in paths {
            if Path::new(path)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
                || path.is_empty()
            {
                return Err("无效文件路径".into());
            }
            let change = status
                .changes
                .iter()
                .find(|c| c.path == *path)
                .ok_or("所选文件状态已改变")?;
            if matches!(action, GitAction::Discard(_)) && (change.untracked || !change.unstaged) {
                return Err("只能撤销已跟踪文件的未暂存修改".into());
            }
            if !matches!(action, GitAction::Discard(_)) {
                if let Some(old) = &change.old_path {
                    if !operation_paths.contains(old) {
                        operation_paths.push(old.clone());
                    }
                }
            }
            // Reject links escaping the project, including linked parent directories.
            let candidate = self.root.join(path);
            if candidate.exists() {
                self.resolve(Path::new(path))?;
            } else {
                let mut parent = Path::new(path).parent().unwrap_or(Path::new(""));
                while !self.root.join(parent).exists() {
                    parent = parent.parent().ok_or("缺少项目根目录")?;
                }
                self.resolve(parent)?;
            }
        }
        let unborn = self.repository()?.head().is_err();
        let mut args = match action {
            GitAction::Stage(_) => vec!["add", "-A", "--"],
            GitAction::Unstage(_) if unborn => vec!["rm", "--cached", "-f", "--"],
            GitAction::Unstage(_) => vec!["restore", "--staged", "--"],
            GitAction::Discard(_) => vec!["restore", "--worktree", "--"],
            GitAction::Commit(_) => unreachable!("handled above"),
        };
        args.extend(operation_paths.iter().map(String::as_str));
        super::process::git(&self.root, &args)
    }
}
