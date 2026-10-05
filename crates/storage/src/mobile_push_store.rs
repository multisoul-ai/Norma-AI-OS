use crate::{migrations::open_app_connection, MobileDeviceGrant, StorageError, StorageResult};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PushEnvironment {
    Sandbox,
    Production,
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MobilePushRegistration {
    pub token: String,
    pub environment: PushEnvironment,
}
impl fmt::Debug for MobilePushRegistration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MobilePushRegistration")
            .field("token", &"[redacted]")
            .field("environment", &self.environment)
            .finish()
    }
}
impl MobilePushRegistration {
    pub fn validate(&self) -> StorageResult<()> {
        // APNs does not guarantee a fixed token size; accept 16–256 bytes as lowercase hex.
        if !(32..=512).contains(&self.token.len())
            || !self.token.len().is_multiple_of(2)
            || !self
                .token
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(invalid("invalid APNs token"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MobilePushState {
    pub revision: u64,
    pub registration: Option<MobilePushRegistration>,
    pub registered_at_ms: Option<u64>,
}

/// Blocking AppDB operations. The caller supplies a freshly authenticated grant, never phone JSON.
#[derive(Clone, Debug)]
pub struct MobilePushStore {
    path: PathBuf,
}
impl MobilePushStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn get(&self, grant: &MobileDeviceGrant) -> StorageResult<Option<MobilePushState>> {
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction()?;
        if !current_grant(&tx, grant)? {
            return Ok(None);
        }
        Ok(Some(read(&tx, &grant.device_id)?))
    }

    /// Compare-and-set prevents delayed enable/disable or token callbacks from overwriting newer intent.
    /// None means the device grant or subscription revision changed; callers must re-read, not replay.
    pub fn replace(
        &self,
        grant: &MobileDeviceGrant,
        expected_revision: u64,
        registration: Option<&MobilePushRegistration>,
    ) -> StorageResult<Option<MobilePushState>> {
        if expected_revision >= i64::MAX as u64 {
            return Err(invalid("push revision exhausted"));
        }
        if let Some(registration) = registration {
            registration.validate()?;
        }
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !current_grant(&tx, grant)? {
            return Ok(None);
        }
        let previous = read(&tx, &grant.device_id)?;
        if previous.revision != expected_revision {
            return Ok(None);
        }
        let time = registration.map(|_| timestamp_ms()).transpose()?;
        tx.execute("INSERT INTO mobile_push_subscriptions(device_id,revision,token,environment,registered_at_ms) VALUES(?1,?2,?3,?4,?5)
            ON CONFLICT(device_id) DO UPDATE SET revision=excluded.revision,token=excluded.token,environment=excluded.environment,registered_at_ms=excluded.registered_at_ms",
            params![grant.device_id, expected_revision + 1, registration.map(|r| &r.token), registration.map(|r| match r.environment { PushEnvironment::Sandbox => "sandbox", PushEnvironment::Production => "production" }), time])?;
        let state = read(&tx, &grant.device_id)?;
        tx.commit()?;
        Ok(Some(state))
    }

    /// APNs invalidation applies only to the exact sent registration and its timestamp.
    pub fn invalidate(
        &self,
        device: &str,
        expected_revision: u64,
        invalidated_at_ms: u64,
    ) -> StorageResult<bool> {
        crate::mobile_access_store::validate_id(device)?;
        if expected_revision == 0
            || expected_revision >= i64::MAX as u64
            || invalidated_at_ms > i64::MAX as u64
        {
            return Err(invalid("invalid APNs invalidation"));
        }
        let connection = open_app_connection(&self.path)?;
        Ok(connection.execute("UPDATE mobile_push_subscriptions SET revision=revision+1,token=NULL,environment=NULL,registered_at_ms=NULL
            WHERE device_id=?1 AND revision=?2 AND token IS NOT NULL AND registered_at_ms<=?3",
            params![device,expected_revision,invalidated_at_ms])? == 1)
    }
}

pub(crate) fn current_grant(
    connection: &Connection,
    grant: &MobileDeviceGrant,
) -> StorageResult<bool> {
    if grant.revoked {
        return Ok(false);
    }
    Ok(connection
        .query_row(
            "SELECT 1 FROM mobile_devices WHERE device_id=?1 AND revision=?2 AND revoked=0",
            params![grant.device_id, grant.revision],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}
fn read(connection: &Connection, device: &str) -> StorageResult<MobilePushState> {
    let row = connection.query_row("SELECT revision,token,environment,registered_at_ms FROM mobile_push_subscriptions WHERE device_id=?1", [device], |row| {
        Ok((row.get::<_,u64>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,Option<u64>>(3)?))
    }).optional()?;
    let Some((revision, token, environment, registered_at_ms)) = row else {
        return Ok(MobilePushState {
            revision: 0,
            registration: None,
            registered_at_ms: None,
        });
    };
    let registration = match (token, environment, registered_at_ms) {
        (None, None, None) => None,
        (Some(token), Some(environment), Some(time)) if time > 0 => {
            let environment = match environment.as_str() {
                "sandbox" => PushEnvironment::Sandbox,
                "production" => PushEnvironment::Production,
                _ => return Err(invalid("invalid push environment")),
            };
            let registration = MobilePushRegistration { token, environment };
            registration.validate()?;
            Some(registration)
        }
        _ => return Err(invalid("invalid push subscription")),
    };
    Ok(MobilePushState {
        revision,
        registration,
        registered_at_ms,
    })
}
fn timestamp_ms() -> StorageResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .filter(|time| *time > 0 && *time <= i64::MAX as u64)
        .ok_or_else(|| invalid("invalid registration clock"))
}
fn invalid(message: &str) -> StorageError {
    StorageError::InvalidMobileAccess(message.into())
}

pub(crate) fn revoke(connection: &Connection, device: &str) -> StorageResult<()> {
    connection.execute("UPDATE mobile_push_subscriptions SET token=NULL,environment=NULL,registered_at_ms=NULL,
        revision=CASE WHEN revision<9223372036854775807 THEN revision+1 ELSE revision END WHERE device_id=?1", [device])?;
    Ok(())
}
pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS mobile_push_subscriptions (
        device_id TEXT PRIMARY KEY NOT NULL REFERENCES mobile_devices(device_id), revision INTEGER NOT NULL CHECK(revision>0),
        token TEXT, environment TEXT CHECK(environment IN ('sandbox','production')), registered_at_ms INTEGER,
        CHECK((token IS NULL AND environment IS NULL AND registered_at_ms IS NULL) OR (token IS NOT NULL AND environment IS NOT NULL AND registered_at_ms IS NOT NULL AND registered_at_ms>0))
    )")?;
    Ok(())
}
