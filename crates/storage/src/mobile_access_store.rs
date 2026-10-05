use crate::orchestration_authorization_store::{constant_time_equal, hash_token};
use crate::{current_unix_timestamp, migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MobilePermission {
    Read,
    Control,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MobileProjectAccess {
    pub workspace_id: String,
    pub access: MobilePermission,
}

/// Desktop-approved scopes. The bearer credential and its hash are deliberately absent.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MobileDeviceGrant {
    pub device_id: String,
    pub device_name: String,
    pub projects: Vec<MobileProjectAccess>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_projects_access: Option<MobilePermission>,
    pub revision: u64,
    pub revoked: bool,
}

impl MobileDeviceGrant {
    /// This is a snapshot, not a lasting authorization lease. Revalidate the current revision
    /// at dispatch so a command queued before a revoke cannot use its previous permissions.
    pub fn allows(&self, workspace_id: &str, control: bool) -> bool {
        !self.revoked
            && self.projects.iter().any(|project| {
                project.workspace_id == workspace_id
                    && (!control || project.access == MobilePermission::Control)
            })
    }
}

/// App-wide device grants, separate from local Provider authorization. All methods perform IO
/// and must run off the GPUI foreground; only explicit desktop pairing can call register/update.
#[derive(Clone, Debug)]
pub struct MobileAccessStore {
    path: PathBuf,
}

impl MobileAccessStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn register(
        &self,
        device_id: &str,
        name: &str,
        token: &str,
        projects: &[MobileProjectAccess],
    ) -> StorageResult<MobileDeviceGrant> {
        self.register_scope(device_id, name, token, projects, None)
    }

    /// Only a desktop approval action may call this; the device cannot request its own scope.
    pub fn register_all_projects(
        &self,
        device_id: &str,
        name: &str,
        token: &str,
        access: MobilePermission,
    ) -> StorageResult<MobileDeviceGrant> {
        self.register_scope(device_id, name, token, &[], Some(access))
    }

    fn register_scope(
        &self,
        device_id: &str,
        name: &str,
        token: &str,
        projects: &[MobileProjectAccess],
        all_projects_access: Option<MobilePermission>,
    ) -> StorageResult<MobileDeviceGrant> {
        validate_id(device_id)?;
        validate_token(token)?;
        if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control)
        {
            return Err(invalid("invalid device name"));
        }
        validate_projects(projects)?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction()?;
        let now = current_unix_timestamp()?;
        // Only explicit desktop approval can revive a revoked ID. Advance its revision so
        // queued commands and permission edits cannot reuse the authorization from before revoke.
        let changed = tx.execute(
            "INSERT INTO mobile_devices
             (device_id,device_name,token_hash,permissions_json,all_projects_access,revision,revoked,created_at_unix,updated_at_unix)
             VALUES (?1,?2,?3,?4,?5,1,0,?6,?6)
             ON CONFLICT(device_id) DO UPDATE SET
             device_name=excluded.device_name,token_hash=excluded.token_hash,
             permissions_json=excluded.permissions_json,all_projects_access=excluded.all_projects_access,
             revision=mobile_devices.revision+1,revoked=0,updated_at_unix=excluded.updated_at_unix
             WHERE mobile_devices.revoked=1 AND mobile_devices.revision < 9223372036854775807",
            params![device_id,name,hash_token(token),serde_json::to_string(projects)?,all_projects_access.map(permission_name),now],
        )?;
        if changed != 1 {
            return Err(invalid("device already paired or grant revision exhausted"));
        }
        let grant = read_grant(&tx, device_id)?
            .ok_or_else(|| invalid("device not found"))?
            .0;
        tx.commit()?;
        Ok(grant)
    }

    pub fn authenticate(
        &self,
        device_id: &str,
        token: &str,
    ) -> StorageResult<Option<MobileDeviceGrant>> {
        validate_id(device_id)?;
        validate_token(token)?;
        let connection = open_app_connection(&self.path)?;
        let Some((grant, stored_hash)) = read_grant(&connection, device_id)? else {
            return Ok(None);
        };
        if grant.revoked
            || !constant_time_equal(stored_hash.as_bytes(), hash_token(token).as_bytes())
        {
            return Ok(None);
        }
        Ok(Some(grant))
    }

    pub fn replace_permissions(
        &self,
        device_id: &str,
        expected_revision: u64,
        projects: &[MobileProjectAccess],
    ) -> StorageResult<MobileDeviceGrant> {
        self.replace_scope(device_id, expected_revision, projects, None)
    }

    pub fn replace_with_all_projects(
        &self,
        device_id: &str,
        expected_revision: u64,
        access: MobilePermission,
    ) -> StorageResult<MobileDeviceGrant> {
        self.replace_scope(device_id, expected_revision, &[], Some(access))
    }

    fn replace_scope(
        &self,
        device_id: &str,
        expected_revision: u64,
        projects: &[MobileProjectAccess],
        all_projects_access: Option<MobilePermission>,
    ) -> StorageResult<MobileDeviceGrant> {
        validate_id(device_id)?;
        validate_projects(projects)?;
        if expected_revision == 0 || expected_revision >= i64::MAX as u64 {
            return Err(invalid("invalid grant revision"));
        }
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction()?;
        let changed = tx.execute(
            "UPDATE mobile_devices SET permissions_json=?1,all_projects_access=?2,revision=revision+1,updated_at_unix=?3
            WHERE device_id=?4 AND revision=?5 AND revoked=0",
            params![
                serde_json::to_string(projects)?,
                all_projects_access.map(permission_name),
                current_unix_timestamp()?,
                device_id,
                expected_revision
            ],
        )?;
        if changed != 1 {
            return Err(invalid("device grant changed or revoked"));
        }
        let grant = read_grant(&tx, device_id)?
            .ok_or_else(|| invalid("device not found"))?
            .0;
        tx.commit()?;
        Ok(grant)
    }

    pub fn revoke(&self, device_id: &str) -> StorageResult<MobileDeviceGrant> {
        validate_id(device_id)?;
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction()?;
        tx.execute(
            "UPDATE mobile_devices SET revoked=1,revision=revision+1,updated_at_unix=?1
            WHERE device_id=?2 AND revoked=0 AND revision < 9223372036854775807",
            params![current_unix_timestamp()?, device_id],
        )?;
        let grant = read_grant(&tx, device_id)?
            .ok_or_else(|| invalid("device not found"))?
            .0;
        if !grant.revoked {
            return Err(invalid("grant revision exhausted"));
        }
        crate::mobile_push_store::revoke(&tx, device_id)?;
        tx.commit()?;
        Ok(grant)
    }

    pub fn list(&self) -> StorageResult<Vec<MobileDeviceGrant>> {
        let connection = open_app_connection(&self.path)?;
        let mut query = connection
            .prepare("SELECT device_id FROM mobile_devices ORDER BY device_name,device_id")?;
        let ids = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        // ponytail: reuse the validated row reader for the small paired-device list;
        // batch the SELECT if large managed device fleets make per-device queries material.
        ids.into_iter()
            .map(|id| {
                read_grant(&connection, &id)?
                    .map(|pair| pair.0)
                    .ok_or_else(|| invalid("device disappeared"))
            })
            .collect()
    }
}

fn read_grant(
    connection: &Connection,
    device_id: &str,
) -> StorageResult<Option<(MobileDeviceGrant, String)>> {
    let row = connection
        .query_row(
            "SELECT device_name,token_hash,permissions_json,all_projects_access,revision,revoked
         FROM mobile_devices WHERE device_id=?1",
            [device_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, u64>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((device_name, hash, stored_projects, access, revision, revoked)) = row else {
        return Ok(None);
    };
    let stored_projects: Vec<MobileProjectAccess> = serde_json::from_str(&stored_projects)?;
    validate_projects(&stored_projects)?;
    let all_projects_access = match access.as_deref() {
        None => None,
        Some("read") => Some(MobilePermission::Read),
        Some("control") => Some(MobilePermission::Control),
        _ => return Err(invalid("invalid all-projects access")),
    };
    if all_projects_access.is_some() && !stored_projects.is_empty() {
        return Err(invalid("mixed project grant modes"));
    }
    let projects = if let Some(access) = all_projects_access {
        let mut query = connection
            .prepare("SELECT workspace_id FROM workspace_identities ORDER BY workspace_id")?;
        let current_projects = query
            .query_map([], |row| {
                Ok(MobileProjectAccess {
                    workspace_id: row.get(0)?,
                    access,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        current_projects
    } else {
        stored_projects
    };
    Ok(Some((
        MobileDeviceGrant {
            device_id: device_id.into(),
            device_name,
            projects,
            all_projects_access,
            revision,
            revoked,
        },
        hash,
    )))
}

fn validate_projects(projects: &[MobileProjectAccess]) -> StorageResult<()> {
    // At most 256 explicitly selected projects per phone. Empty grants can pair an empty Mac
    // but grant no project access; adding a project later still needs desktop authorization.
    if projects.len() > 256 {
        return Err(invalid("too many project grants"));
    }
    let mut seen = HashSet::new();
    for project in projects {
        validate_id(&project.workspace_id)?;
        if !seen.insert(&project.workspace_id) {
            return Err(invalid("duplicate project grant"));
        }
    }
    Ok(())
}

pub(crate) fn validate_id(id: &str) -> StorageResult<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(invalid("invalid device or workspace identity"));
    }
    Ok(())
}

fn validate_token(token: &str) -> StorageResult<()> {
    // 32 random credential bytes are transported as 64 hex characters; never log their value.
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("invalid device credential"));
    }
    Ok(())
}

fn permission_name(access: MobilePermission) -> &'static str {
    match access {
        MobilePermission::Read => "read",
        MobilePermission::Control => "control",
    }
}

fn invalid(message: &str) -> StorageError {
    StorageError::InvalidMobileAccess(message.into())
}

pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS mobile_devices (
        device_id TEXT PRIMARY KEY NOT NULL,device_name TEXT NOT NULL,token_hash TEXT NOT NULL,
        permissions_json TEXT NOT NULL,
        all_projects_access TEXT CHECK(all_projects_access IN ('read','control')),
        revision INTEGER NOT NULL CHECK(revision>0),
        revoked INTEGER NOT NULL CHECK(revoked IN (0,1)),created_at_unix INTEGER NOT NULL,updated_at_unix INTEGER NOT NULL
    );")?;
    crate::migrations::ensure_column(
        connection,
        "mobile_devices",
        "all_projects_access",
        "all_projects_access TEXT CHECK(all_projects_access IN ('read','control'))",
    )?;
    Ok(())
}
