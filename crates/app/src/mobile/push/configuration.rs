//! One atomic Keychain item binds the endpoint, environment and relay credential.
use super::PushRelayConfig;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use storage::{AppStore, PushEnvironment};

#[cfg(all(target_os = "macos", feature = "l3-test-support"))]
#[path = "../../../tests/support/mobile_push_keychain.rs"]
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
    let account = store.mobile_access_store().keychain_account()?;
    let Some(bytes) = read(&account)? else {
        return Ok(None);
    };
    ensure!(
        bytes.len() <= 4096,
        "Saved notification settings are too large"
    );
    let saved: Option<Saved> = serde_json::from_slice(&bytes)
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
    let account = store.mobile_access_store().keychain_account()?;
    write(&account, &serde_json::to_vec(&saved)?)
}

#[cfg(not(any(test, feature = "e2e-test-support")))]
fn read(account: &str) -> Result<Option<Vec<u8>>> {
    match security_framework::passwords::get_generic_password(
        "com.norma-ai.os.mobile-push",
        account,
    ) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.code() == -25300 => Ok(None), // errSecItemNotFound; other Keychain errors are not an empty config.
        Err(_) => anyhow::bail!(
            "Unable to read the notification service Keychain item. Unlock Keychain and try again."
        ),
    }
}
#[cfg(not(any(test, feature = "e2e-test-support")))]
fn write(account: &str, bytes: &[u8]) -> Result<()> {
    security_framework::passwords::set_generic_password(
        "com.norma-ai.os.mobile-push",
        account,
        bytes,
    )
    .map_err(|_| anyhow::anyhow!("Unable to save the notification service Keychain item. Previous settings are unchanged."))
}

#[cfg(any(test, feature = "e2e-test-support"))]
fn memory() -> &'static std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>> {
    static VALUES: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
    > = std::sync::OnceLock::new();
    VALUES.get_or_init(Default::default)
}
#[cfg(any(test, feature = "e2e-test-support"))]
fn read(account: &str) -> Result<Option<Vec<u8>>> {
    Ok(memory()
        .lock()
        .map_err(|_| anyhow::anyhow!("test vault poisoned"))?
        .get(account)
        .cloned())
}
#[cfg(any(test, feature = "e2e-test-support"))]
fn write(account: &str, bytes: &[u8]) -> Result<()> {
    memory()
        .lock()
        .map_err(|_| anyhow::anyhow!("test vault poisoned"))?
        .insert(account.into(), bytes.into());
    Ok(())
}
