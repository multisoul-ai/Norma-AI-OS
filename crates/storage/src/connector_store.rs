//! Connector settings, credential digests and bounded audit history. Never stores secrets.
use crate::{migrations::open_app_connection, StorageResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ConnectorConfig {
    pub enabled: bool,
    pub port: u16,
    pub tunnel_authority: Option<workspace::HostAuthority>,
}
impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 47615,
            tunnel_authority: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenHash {
    pub hash: [u8; 32],
    pub expires_at: Option<i64>,
}
#[derive(Clone, Debug)]
pub struct ConnectorAudit {
    pub occurred_at: i64,
    pub tool: String,
    pub result_code: String,
    pub failure_count: u64,
}
#[derive(Clone)]
pub struct ConnectorStore {
    path: PathBuf,
}
impl ConnectorStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn config(&self) -> StorageResult<ConnectorConfig> {
        crate::SettingsStore::new(self.path.clone())
            .get_string("connector.config.v1")?
            .map(|s| serde_json::from_str(&s).map_err(Into::into))
            .unwrap_or_else(|| Ok(ConnectorConfig::default()))
    }
    pub fn save_config(&self, config: &ConnectorConfig) -> StorageResult<()> {
        crate::SettingsStore::new(self.path.clone())
            .set_string("connector.config.v1", &serde_json::to_string(config)?)
    }
    pub fn tokens(&self) -> StorageResult<Vec<TokenHash>> {
        let conn = open_app_connection(&self.path)?;
        let mut stmt =
            conn.prepare("SELECT token_hash,expires_at_unix FROM connector_token_hashes")?;
        let rows = stmt
            .query_map([], |row| {
                let bytes: Vec<u8> = row.get(0)?;
                let hash = bytes
                    .try_into()
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                Ok(TokenHash {
                    hash,
                    expires_at: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
    pub fn replace_tokens(&self, tokens: &[TokenHash]) -> StorageResult<()> {
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM connector_token_hashes", [])?;
        for token in tokens {
            tx.execute(
                "INSERT INTO connector_token_hashes(token_hash,expires_at_unix) VALUES(?1,?2)",
                params![token.hash.as_slice(), token.expires_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    /// Call only on a background worker. Parameter bodies and credentials never enter this API.
    pub fn audit(&self, now: i64, tool: &str, result: &str) -> StorageResult<()> {
        self.audit_parameters(now, tool, "", result)
    }
    pub fn audit_parameters(
        &self,
        now: i64,
        tool: &str,
        parameters: &str,
        result: &str,
    ) -> StorageResult<()> {
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM connector_audit WHERE occurred_at_unix < ?1",
            [now.saturating_sub(90 * 86400)],
        )?;
        tx.execute("INSERT INTO connector_audit(occurred_at_unix,tool,parameter_summary,result_code) VALUES(?1,?2,?3,?4)", params![now, tool, parameters, result])?;
        tx.commit()?;
        Ok(())
    }
    /// One row per canonical source IP and failure window, including requests during lockout.
    pub fn audit_auth_failure(
        &self,
        now: i64,
        source: &str,
        window_start: i64,
        count: u64,
        locked_until: i64,
    ) -> StorageResult<()> {
        let mut conn = open_app_connection(&self.path)?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM connector_audit WHERE occurred_at_unix < ?1",
            [now.saturating_sub(90 * 86400)],
        )?;
        let operation = format!("auth:{source}:{window_start}");
        let summary = serde_json::json!({"source_ip":source,"failure_count":count,"locked_until":locked_until}).to_string();
        tx.execute("INSERT INTO connector_audit(occurred_at_unix,tool,parameter_summary,operation_id,result_code) VALUES(?1,'authentication',?2,?3,'unauthorized')
            ON CONFLICT(operation_id) WHERE tool='authentication' AND operation_id IS NOT NULL
            DO UPDATE SET occurred_at_unix=excluded.occurred_at_unix,parameter_summary=excluded.parameter_summary", params![now, summary, operation])?;
        tx.commit()?;
        Ok(())
    }
    pub fn audit_page(&self, limit: usize) -> StorageResult<Vec<ConnectorAudit>> {
        let conn = open_app_connection(&self.path)?;
        let mut stmt = conn.prepare("SELECT occurred_at_unix,tool,result_code,parameter_summary FROM connector_audit ORDER BY sequence DESC LIMIT ?1")?;
        let rows = stmt
            .query_map([limit.min(50)], |r| {
                Ok(ConnectorAudit {
                    occurred_at: r.get(0)?,
                    tool: r.get(1)?,
                    result_code: r.get(2)?,
                    failure_count: serde_json::from_str::<serde_json::Value>(
                        &r.get::<_, String>(3)?,
                    )
                    .ok()
                    .and_then(|v| v["failure_count"].as_u64())
                    .unwrap_or(1),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
pub(crate) fn migrate(conn: &Connection) -> StorageResult<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS connector_token_hashes(token_hash BLOB PRIMARY KEY NOT NULL CHECK(length(token_hash)=32),expires_at_unix INTEGER);
        CREATE TABLE IF NOT EXISTS connector_audit(sequence INTEGER PRIMARY KEY AUTOINCREMENT,occurred_at_unix INTEGER NOT NULL,tool TEXT NOT NULL,parameter_summary TEXT NOT NULL,operation_id TEXT,result_code TEXT NOT NULL,user_quote TEXT);
        CREATE INDEX IF NOT EXISTS connector_audit_time ON connector_audit(occurred_at_unix);
        CREATE UNIQUE INDEX IF NOT EXISTS connector_auth_window ON connector_audit(operation_id) WHERE tool='authentication' AND operation_id IS NOT NULL;")?;
    Ok(())
}
