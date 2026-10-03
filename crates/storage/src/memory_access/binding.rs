use super::journal::failure;
use crate::migrations::open_app_connection;
use agent::{AdapterError, MemoryScope, MemoryScopeAlias};
use rusqlite::OptionalExtension;
use std::path::Path;

#[derive(Clone)]
pub(super) struct Binding {
    pub project: String,
    pub worktree: String,
    signature: String,
}

fn signature(path: &Path) -> Result<String, AdapterError> {
    let meta = std::fs::metadata(path).map_err(failure)?;
    if !meta.is_dir() {
        return Err(AdapterError::Rejected("项目身份必须是目录".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // Birth time distinguishes a replacement even when an inode is recycled.
        let birth = meta
            .created()
            .map_err(failure)?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(failure)?
            .as_nanos();
        Ok(format!(
            "{}:{}:{}:{birth}",
            path.display(),
            meta.dev(),
            meta.ino()
        ))
    }
    #[cfg(not(unix))]
    {
        Err(AdapterError::Rejected(
            "当前平台尚不能核验项目目录身份".into(),
        ))
    }
}

fn identities(root: &Path) -> Result<(String, String), AdapterError> {
    let identity = workspace::project::identify_project(root).map_err(failure)?;
    Ok((signature(&identity.key)?, signature(&identity.root)?))
}

fn resolve(path: &Path, identity: &str) -> Result<String, AdapterError> {
    let connection = open_app_connection(path).map_err(failure)?;
    let id = crate::workspace_identity_store::random_workspace_id().map_err(failure)?;
    connection
        .execute(
            "INSERT OR IGNORE INTO memory_scope_bindings VALUES (?1,?2)",
            rusqlite::params![identity, id],
        )
        .map_err(failure)?;
    connection
        .query_row(
            "SELECT memory_id FROM memory_scope_bindings WHERE identity=?1",
            [identity],
            |row| row.get(0),
        )
        .optional()
        .map_err(failure)?
        .ok_or_else(|| failure("缺少记忆归属"))
}

impl Binding {
    pub fn open(database: &Path, root: &Path) -> Result<Self, AdapterError> {
        let (project, worktree) = identities(root)?;
        Ok(Self {
            project: resolve(database, &project)?,
            worktree: resolve(database, &worktree)?,
            signature: format!("{project}|{worktree}"),
        })
    }
    pub fn verify(&self, root: &Path) -> Result<(), AdapterError> {
        let (project, worktree) = identities(root)?;
        if self.signature != format!("{project}|{worktree}") {
            return Err(AdapterError::Rejected(
                "项目目录身份已变化，请重新打开会话".into(),
            ));
        }
        Ok(())
    }
    pub fn scope(&self, alias: MemoryScopeAlias) -> MemoryScope {
        match alias {
            MemoryScopeAlias::Personal => MemoryScope::Personal,
            MemoryScopeAlias::Project => MemoryScope::ProjectShared {
                project_id: self.project.clone(),
            },
            MemoryScopeAlias::Worktree => MemoryScope::Worktree {
                project_id: self.project.clone(),
                worktree_id: self.worktree.clone(),
            },
        }
    }
}
