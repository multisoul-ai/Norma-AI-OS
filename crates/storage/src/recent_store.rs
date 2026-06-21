use crate::{current_unix_timestamp, migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::params;
use std::path::{Path, PathBuf};
use workspace::{
    recent::canonical_workspace_path, RecentWorkspaces, WorkspaceRecord, WorkspaceSource,
};

#[derive(Clone, Debug)]
pub struct RecentStore {
    path: PathBuf,
}

impl RecentStore {
    pub fn new(path: PathBuf) -> Self {
        RecentStore { path }
    }

    pub fn open_default() -> StorageResult<Self> {
        Ok(Self::new(crate::default_app_db_path()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> StorageResult<RecentWorkspaces> {
        let connection = open_app_connection(&self.path)?;
        let mut statement = connection.prepare(
            r#"
            SELECT path, display_name, last_opened_at_unix, source
            FROM recent_workspaces
            ORDER BY last_opened_at_unix DESC, display_name ASC, path ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut records = Vec::new();
        for row in rows {
            let (path, display_name, last_opened_at_unix, source) = row?;
            records.push(WorkspaceRecord {
                path: PathBuf::from(path),
                display_name,
                last_opened_at_unix,
                source: decode_source(&source)?,
            });
        }
        Ok(RecentWorkspaces { records })
    }

    pub fn save(&self, recent: &RecentWorkspaces) -> StorageResult<()> {
        let mut connection = open_app_connection(&self.path)?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM recent_workspaces", [])?;
        for record in &recent.records {
            transaction.execute(
                r#"
                INSERT INTO recent_workspaces (
                    path, display_name, last_opened_at_unix, source
                ) VALUES (?1, ?2, ?3, ?4)
                "#,
                params![
                    record.path.to_string_lossy(),
                    record.display_name,
                    record.last_opened_at_unix,
                    encode_source(record.source),
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn record_opened(
        &self,
        path: &Path,
        source: WorkspaceSource,
    ) -> StorageResult<RecentWorkspaces> {
        if !path.is_dir() {
            return Err(StorageError::InvalidWorkspacePath(path.to_path_buf()));
        }

        let canonical_path = canonical_workspace_path(path)?;
        let mut recent = self.load()?;
        recent.record_opened_at(canonical_path, source, current_unix_timestamp()?);
        self.save(&recent)?;
        Ok(recent)
    }
}

fn encode_source(source: WorkspaceSource) -> &'static str {
    match source {
        WorkspaceSource::Open => "open",
        WorkspaceSource::Clone => "clone",
    }
}

fn decode_source(source: &str) -> StorageResult<WorkspaceSource> {
    match source {
        "open" => Ok(WorkspaceSource::Open),
        "clone" => Ok(WorkspaceSource::Clone),
        other => Err(StorageError::InvalidWorkspaceSource(other.to_string())),
    }
}

#[cfg(test)]
#[path = "recent_store_tests.rs"]
mod tests;
