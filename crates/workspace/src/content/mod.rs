//! Project-scoped file browsing and local Git operations. Call from a background executor.
use std::path::{Component, Path, PathBuf};

mod atomic;
mod files;
mod git;
mod mutation;
mod process;
pub use files::{FileEntry, FileOptions, TextFile, MAX_TEXT_BYTES};
pub use git::{Change, Comparison, GitSnapshot};
pub use mutation::GitAction;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug)]
pub struct Project {
    root: PathBuf,
}

impl Project {
    pub fn open(root: &Path) -> Result<Self> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if !root.is_dir() {
            return Err("项目目录不存在".into());
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, relative: &Path) -> Result<PathBuf> {
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("文件路径必须位于项目内".into());
        }
        if relative.components().any(|c| c.as_os_str() == ".git") {
            return Err("不允许访问 Git 内部文件".into());
        }
        let path = self
            .root
            .join(relative)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !path.starts_with(&self.root)
            || path
                .strip_prefix(&self.root)
                .is_ok_and(|p| p.components().any(|c| c.as_os_str() == ".git"))
        {
            return Err("文件链接指向项目外部".into());
        }
        Ok(path)
    }

    pub(super) fn repository(&self) -> Result<git2::Repository> {
        let repo = git2::Repository::discover(&self.root).map_err(|e| e.message().to_string())?;
        let work = repo.workdir().ok_or("裸仓库不支持文件工作台")?;
        if work.canonicalize().map_err(|e| e.to_string())? != self.root {
            return Err("请打开 Git 工作树根目录以查看和操作变更".into());
        }
        Ok(repo)
    }
}
