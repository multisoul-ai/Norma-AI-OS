use crate::{current_unix_timestamp, migrations::open_app_connection, StorageError, StorageResult};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestrationAuthorizationRecord {
    pub provider: String,
    pub token_hash: String,
    pub enabled: bool,
    pub installed_version: String,
    pub created_at_unix: u64,
    pub updated_at_unix: u64,
    pub revoked_at_unix: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OrchestrationAuthorizationCache {
    records: Vec<OrchestrationAuthorizationRecord>,
}

impl OrchestrationAuthorizationCache {
    pub fn new(records: Vec<OrchestrationAuthorizationRecord>) -> Self {
        Self { records }
    }

    pub fn is_authorized(&self, provider: &str, token: &str) -> bool {
        let candidate_hash = hash_token(token);
        self.records.iter().any(|record| {
            record.provider == provider
                && record.enabled
                && constant_time_equal(record.token_hash.as_bytes(), candidate_hash.as_bytes())
        })
    }
}

#[derive(Clone, Debug)]
pub struct OrchestrationAuthorizationStore {
    path: PathBuf,
}

impl OrchestrationAuthorizationStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn authorize(
        &self,
        provider: &str,
        token: &str,
        installed_version: &str,
    ) -> StorageResult<()> {
        validate_provider(provider)?;
        let connection = open_app_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        let token_hash = hash_token(token);
        connection.execute(
            "INSERT INTO orchestration_authorizations
             (provider, token_hash, enabled, installed_version, created_at_unix,
              updated_at_unix, revoked_at_unix)
             VALUES (?1, ?2, 1, ?3, ?4, ?4, NULL)
             ON CONFLICT(provider) DO UPDATE SET
               token_hash = excluded.token_hash,
               enabled = 1,
               installed_version = excluded.installed_version,
               updated_at_unix = excluded.updated_at_unix,
               revoked_at_unix = NULL",
            params![provider, token_hash, installed_version, now,],
        )?;
        Ok(())
    }

    pub fn revoke(&self, provider: &str) -> StorageResult<()> {
        validate_provider(provider)?;
        let connection = open_app_connection(&self.path)?;
        let now = current_unix_timestamp()?;
        connection.execute(
            "UPDATE orchestration_authorizations
             SET enabled = 0, updated_at_unix = ?1, revoked_at_unix = ?1
             WHERE provider = ?2",
            params![now, provider],
        )?;
        Ok(())
    }

    pub fn is_authorized(&self, provider: &str, token: &str) -> StorageResult<bool> {
        validate_provider(provider)?;
        let connection = open_app_connection(&self.path)?;
        let stored = connection
            .query_row(
                "SELECT token_hash FROM orchestration_authorizations
                 WHERE provider = ?1 AND enabled = 1",
                [provider],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(stored) = stored else {
            return Ok(false);
        };
        Ok(constant_time_equal(
            stored.as_bytes(),
            hash_token(token).as_bytes(),
        ))
    }

    pub fn list(&self) -> StorageResult<Vec<OrchestrationAuthorizationRecord>> {
        let connection = open_app_connection(&self.path)?;
        let mut statement = connection.prepare(
            "SELECT provider, token_hash, enabled, installed_version, created_at_unix,
                    updated_at_unix, revoked_at_unix
             FROM orchestration_authorizations ORDER BY provider",
        )?;
        let rows = statement.query_map([], |row| {
            let provider: String = row.get(0)?;
            Ok(OrchestrationAuthorizationRecord {
                provider: decode_provider(&provider)?,
                token_hash: row.get(1)?,
                enabled: row.get::<_, i64>(2)? != 0,
                installed_version: row.get(3)?,
                created_at_unix: row.get(4)?,
                updated_at_unix: row.get(5)?,
                revoked_at_unix: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

pub(crate) fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let compared_len = left.len().max(right.len());
    for index in 0..compared_len {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

fn decode_provider(value: &str) -> rusqlite::Result<String> {
    match value {
        "claude" | "codex" | "cursor" | "opencode" | "infcode" | "grok" | "kimi" | "zcode"
        | "pi" | "hermes" => Ok(value.to_string()),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            other.len(),
            rusqlite::types::Type::Text,
            Box::new(StorageError::InvalidAgentDispatchValue(other.to_string())),
        )),
    }
}

fn validate_provider(value: &str) -> StorageResult<()> {
    if matches!(
        value,
        "claude"
            | "codex"
            | "cursor"
            | "opencode"
            | "infcode"
            | "grok"
            | "kimi"
            | "zcode"
            | "pi"
            | "hermes"
    ) {
        Ok(())
    } else {
        Err(StorageError::InvalidAgentDispatchValue(value.to_string()))
    }
}

#[cfg(test)]
#[path = "../tests/unit/orchestration_authorization_tests.rs"]
mod tests;
