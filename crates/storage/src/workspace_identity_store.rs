use crate::{current_unix_timestamp, migrations::open_app_connection, StorageResult};
use rusqlite::{params, OptionalExtension, Row};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceIdentity {
    pub workspace_id: String,
    pub canonical_root: PathBuf,
    pub display_name: String,
    pub created_at_unix: u64,
    pub updated_at_unix: u64,
}

#[derive(Clone, Debug)]
pub struct WorkspaceIdentityStore {
    path: PathBuf,
}

impl WorkspaceIdentityStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn list(&self) -> StorageResult<Vec<WorkspaceIdentity>> {
        let connection = open_app_connection(&self.path)?;
        let mut query = connection.prepare(
            "SELECT workspace_id, canonical_root, display_name, created_at_unix, updated_at_unix
             FROM workspace_identities ORDER BY display_name, workspace_id",
        )?;
        let rows = query.query_map([], decode_identity)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn resolve_or_create(
        &self,
        workspace_root: &Path,
        display_name: &str,
    ) -> StorageResult<WorkspaceIdentity> {
        let canonical_root = std::fs::canonicalize(workspace_root)?;
        let canonical_text = canonical_root.to_string_lossy().into_owned();
        let connection = open_app_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        let existing = select_by_root(&connection, &canonical_text)?;
        if let Some(existing) = existing {
            connection.execute(
                "UPDATE workspace_identities
                 SET display_name = ?1, updated_at_unix = ?2
                 WHERE workspace_id = ?3",
                params![display_name, now, existing.workspace_id],
            )?;
            return select_by_root(&connection, &canonical_text)?
                .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into());
        }

        let workspace_id = random_workspace_id()?;
        connection.execute(
            "INSERT INTO workspace_identities
             (workspace_id, canonical_root, display_name, created_at_unix, updated_at_unix)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![workspace_id, canonical_text, display_name, now],
        )?;
        select_by_root(&connection, &canonical_text)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    pub fn get(&self, workspace_id: &str) -> StorageResult<Option<WorkspaceIdentity>> {
        let connection = open_app_connection(&self.path)?;
        connection
            .query_row(
                "SELECT workspace_id, canonical_root, display_name,
                        created_at_unix, updated_at_unix
                 FROM workspace_identities WHERE workspace_id = ?1",
                [workspace_id],
                decode_identity,
            )
            .optional()
            .map_err(Into::into)
    }
}

fn select_by_root(
    connection: &rusqlite::Connection,
    canonical_root: &str,
) -> StorageResult<Option<WorkspaceIdentity>> {
    connection
        .query_row(
            "SELECT workspace_id, canonical_root, display_name,
                    created_at_unix, updated_at_unix
             FROM workspace_identities WHERE canonical_root = ?1",
            [canonical_root],
            decode_identity,
        )
        .optional()
        .map_err(Into::into)
}

fn decode_identity(row: &Row<'_>) -> rusqlite::Result<WorkspaceIdentity> {
    Ok(WorkspaceIdentity {
        workspace_id: row.get(0)?,
        canonical_root: PathBuf::from(row.get::<_, String>(1)?),
        display_name: row.get(2)?,
        created_at_unix: row.get(3)?,
        updated_at_unix: row.get(4)?,
    })
}

fn random_workspace_id() -> StorageResult<String> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    // RFC 4122 v4: version nibble = 0100, variant high bits = 10.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

#[cfg(test)]
#[path = "../tests/unit/workspace_identity_store_tests.rs"]
mod tests;
