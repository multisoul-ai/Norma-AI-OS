//! One SQLite setting binds the endpoint, environment and relay credential.
use super::PushRelayConfig;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use storage::{AppStore, PushEnvironment};

#[cfg(all(target_os = "macos", feature = "l3-test-support"))]
#[path = "../../../tests/support/mobile_push_sqlite.rs"]
pub mod l3;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    endpoint: String,
    key: String,
    environment: PushEnvironment,
}

pub(crate) struct Summary {
    pub endpoint: Option<String>,
    pub environment: PushEnvironment,
}
impl Summary {
    pub fn from_config(config: Option<&PushRelayConfig>) -> Self {
        Self {
            endpoint: config.map(|c| c.endpoint.to_string()),
            environment: config.map_or(PushEnvironment::Sandbox, |c| c.environment),
        }
    }
}

pub(crate) fn effective(store: &AppStore) -> Result<Option<PushRelayConfig>> {
    match load(store)? {
        Some(saved) => Ok(saved), // JSON null explicitly disables startup environment configuration.
        None => PushRelayConfig::from_environment(),
    }
}

pub(super) fn load(store: &AppStore) -> Result<Option<Option<PushRelayConfig>>> {
    let Some(value) = store.settings_store().get_string("mobile.push.config")? else {
        return Ok(None);
    };
    ensure!(
        value.len() <= 4096,
        "Saved notification settings are too large"
    );
    let saved: Option<Saved> = serde_json::from_str(&value)
        .context("Notification settings are invalid. Save again or disable the service.")?;
    saved
        .map(|s| PushRelayConfig::from_fields(&s.endpoint, &s.key, s.environment))
        .transpose()
        .map(Some)
}

pub(crate) fn save(store: &AppStore, config: Option<&PushRelayConfig>) -> Result<()> {
    let saved = config.map(|c| Saved {
        endpoint: c.endpoint.to_string(),
        key: c.key.clone(),
        environment: c.environment,
    });
    store
        .settings_store()
        .set_string("mobile.push.config", &serde_json::to_string(&saved)?)?;
    Ok(())
}
