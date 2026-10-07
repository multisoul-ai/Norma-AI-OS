//! ActivityKit tokens and durable send watermarks, separate from ordinary alert subscriptions.
use crate::{
    migrations::open_app_connection, MobileDeviceGrant, PushEnvironment, StorageError,
    StorageResult,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, io, path::PathBuf};

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MobileActivityRegistration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_workspace_id: Option<String>,
    pub generation: String,
    pub environment: PushEnvironment,
    pub auto_start: bool,
    pub push_to_start_token: Option<String>,
    pub activity_token: Option<String>,
    pub activity_id: Option<String>,
    pub cycle_id: Option<String>,
    pub dismissed: bool,
}
impl fmt::Debug for MobileActivityRegistration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MobileActivityRegistration")
            .field("tokens", &"[redacted]")
            .finish_non_exhaustive()
    }
}
impl MobileActivityRegistration {
    pub fn validate(&self) -> StorageResult<()> {
        for id in self
            .desktop_workspace_id
            .iter()
            .chain(std::iter::once(&self.generation))
        {
            crate::mobile_access_store::validate_id(id)?;
        }
        for id in [&self.activity_id, &self.cycle_id].into_iter().flatten() {
            crate::mobile_access_store::validate_id(id)?;
        }
        for token in [&self.push_to_start_token, &self.activity_token]
            .into_iter()
            .flatten()
        {
            if !(32..=512).contains(&token.len())
                || !token.len().is_multiple_of(2)
                || !token
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid("invalid activity token"));
            }
        }
        if self.activity_token.is_some() && (self.activity_id.is_none() || self.cycle_id.is_none())
        {
            return Err(invalid("activity identity required"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ActivityJournal {
    pub cycle_id: Option<String>,
    pub started_at: Option<u64>,
    pub ended_at: Option<u64>,
    pub last_sent_at: u64,
    pub quiet_seen: bool,
    pub summary: Option<Value>,
    #[serde(default)]
    pub alerted: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct MobileActivityState {
    pub revision: u64,
    pub registration: Option<MobileActivityRegistration>,
    pub journal: ActivityJournal,
}
#[derive(Clone, Debug)]
pub struct MobileActivityStore {
    path: PathBuf,
}
impl MobileActivityStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn get(&self, grant: &MobileDeviceGrant) -> StorageResult<Option<MobileActivityState>> {
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction()?;
        if !crate::mobile_push_store::current_grant(&tx, grant)? {
            return Ok(None);
        }
        read(&tx, &grant.device_id).map(Some)
    }
    pub fn replace(
        &self,
        grant: &MobileDeviceGrant,
        expected: u64,
        registration: Option<&MobileActivityRegistration>,
    ) -> StorageResult<Option<MobileActivityState>> {
        if expected >= i64::MAX as u64 {
            return Err(invalid("activity revision exhausted"));
        }
        if let Some(value) = registration {
            value.validate()?;
        }
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !crate::mobile_push_store::current_grant(&tx, grant)? {
            return Ok(None);
        }
        let old = read(&tx, &grant.device_id)?;
        if old.revision != expected {
            return Ok(None);
        }
        let same = old
            .registration
            .as_ref()
            .zip(registration)
            .is_some_and(|(a, b)| {
                a.generation == b.generation && a.desktop_workspace_id == b.desktop_workspace_id
            });
        let new_cycle = registration.is_some_and(|r| r.activity_token.is_some())
            && same
            && old.journal.cycle_id.is_some()
            && old.journal.cycle_id.as_ref() != registration.and_then(|r| r.cycle_id.as_ref());
        if new_cycle && !(old.journal.ended_at.is_some() && old.journal.quiet_seen) {
            return Err(invalid("activity cycle changed"));
        }
        let mut journal = if same || registration.is_none() {
            old.journal.clone()
        } else {
            ActivityJournal {
                alerted: old.journal.alerted,
                ..ActivityJournal::default()
            }
        };
        if registration.and_then(|r| r.activity_token.as_ref())
            != old
                .registration
                .as_ref()
                .and_then(|r| r.activity_token.as_ref())
        {
            journal.last_sent_at = 0;
        }
        if new_cycle {
            journal = ActivityJournal {
                alerted: journal.alerted,
                ..ActivityJournal::default()
            };
        }
        if let Some(r) = registration.filter(|r| r.activity_token.is_some()) {
            if journal.cycle_id.is_none() {
                journal.cycle_id = r.cycle_id.clone();
                journal.started_at = Some(crate::current_unix_timestamp()?);
            }
        }
        let next = MobileActivityState {
            revision: expected + 1,
            registration: registration.cloned(),
            journal,
        };
        save(&tx, &grant.device_id, &next)?;
        tx.commit()?;
        Ok(Some(next))
    }
    /// Real idle clears dismissal and the previous update token, permitting the next work cycle.
    pub fn observe_idle(
        &self,
        grant: &MobileDeviceGrant,
        expected: u64,
        at: u64,
    ) -> StorageResult<Option<MobileActivityState>> {
        if expected >= i64::MAX as u64 {
            return Err(invalid("activity revision exhausted"));
        }
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !crate::mobile_push_store::current_grant(&tx, grant)? {
            return Ok(None);
        }
        let mut state = read(&tx, &grant.device_id)?;
        if state.revision != expected {
            return Ok(None);
        }
        state.revision += 1;
        state.journal.ended_at = state.journal.ended_at.or(Some(at));
        state.journal.quiet_seen = true;
        if let Some(r) = &mut state.registration {
            r.dismissed = false;
            r.activity_token = None;
            r.activity_id = None;
            r.cycle_id = None;
        }
        save(&tx, &grant.device_id, &state)?;
        tx.commit()?;
        Ok(Some(state))
    }
    /// Reserve a start before HTTP. A crash or unknown APNs result never replays that start.
    pub fn update_journal(
        &self,
        grant: &MobileDeviceGrant,
        expected: u64,
        journal: &ActivityJournal,
    ) -> StorageResult<bool> {
        if journal.alerted.len() > 4096 || serde_json::to_vec(journal)?.len() > 512 * 1024 {
            return Err(invalid("activity journal too large"));
        }
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !crate::mobile_push_store::current_grant(&tx, grant)? {
            return Ok(false);
        }
        let mut state = read(&tx, &grant.device_id)?;
        if state.revision != expected {
            return Ok(false);
        }
        state.journal = journal.clone();
        save(&tx, &grant.device_id, &state)?;
        tx.commit()?;
        Ok(true)
    }
    pub fn invalidate(
        &self,
        grant: &MobileDeviceGrant,
        expected: u64,
        token: &str,
    ) -> StorageResult<bool> {
        let Some(state) = self.get(grant)? else {
            return Ok(false);
        };
        if state.revision != expected {
            return Ok(false);
        }
        let Some(mut registration) = state.registration else {
            return Ok(false);
        };
        if registration.activity_token.as_deref() == Some(token) {
            registration.activity_token = None;
        } else if registration.push_to_start_token.as_deref() == Some(token) {
            registration.push_to_start_token = None;
        } else {
            return Ok(false);
        }
        self.replace(grant, expected, Some(&registration))
            .map(|r| r.is_some())
    }
}
fn read(connection: &Connection, device: &str) -> StorageResult<MobileActivityState> {
    let saved: Option<String> = connection
        .query_row(
            "SELECT data_json FROM mobile_activity_subscriptions WHERE device_id=?1",
            [device],
            |r| r.get(0),
        )
        .optional()?;
    saved
        .map(|s| serde_json::from_str(&s).map_err(StorageError::from))
        .unwrap_or_else(|| {
            Ok(MobileActivityState {
                revision: 0,
                registration: None,
                journal: ActivityJournal::default(),
            })
        })
}
fn save(connection: &Connection, device: &str, state: &MobileActivityState) -> StorageResult<()> {
    connection.execute("INSERT INTO mobile_activity_subscriptions(device_id,data_json) VALUES(?1,?2) ON CONFLICT(device_id) DO UPDATE SET data_json=excluded.data_json", params![device,serde_json::to_string(state)?])?;
    Ok(())
}
pub(crate) fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS mobile_activity_subscriptions(device_id TEXT PRIMARY KEY NOT NULL REFERENCES mobile_devices(device_id),data_json TEXT NOT NULL)")?;
    Ok(())
}
fn invalid(message: &str) -> StorageError {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}
