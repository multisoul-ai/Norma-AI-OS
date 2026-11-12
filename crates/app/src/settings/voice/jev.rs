#[cfg(test)]
#[path = "../../../tests/unit/voice_jev_configuration_tests.rs"]
mod tests;
use agent::jev_choice::{JevChoiceClient, JevChoiceRequest, JevError};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use storage::SettingsStore;

pub(crate) const JEV_CREDENTIAL_REF: &str = "voice.jev.credential_ref.v1";
const PENDING_CLEANUP: &str = "voice.jev.pending_cleanup.v1";
static CONFIGURATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "com.norma.os.voice.jev";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum JevConfigurationError {
    InvalidKey,
    SecureStorage,
    SettingsStorage,
    Network,
    Timeout,
    Http(u16),
    InvalidResponse,
}

pub(crate) trait SecretVault: Send + Sync {
    fn set(&self, account: &str, secret: &str) -> Result<(), JevConfigurationError>;
    fn get(&self, account: &str) -> Result<Option<String>, JevConfigurationError>;
    fn delete(&self, account: &str) -> Result<(), JevConfigurationError>;
}

pub(crate) struct KeychainVault;

#[cfg(target_os = "macos")]
impl SecretVault for KeychainVault {
    fn set(&self, account: &str, secret: &str) -> Result<(), JevConfigurationError> {
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            account,
            secret.as_bytes(),
        )
        .map_err(|_| JevConfigurationError::SecureStorage)
    }
    fn get(&self, account: &str) -> Result<Option<String>, JevConfigurationError> {
        match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, account) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| JevConfigurationError::SecureStorage),
            // macOS errSecItemNotFound. Missing entries are not usable configuration.
            Err(error) if error.code() == -25300 => Ok(None),
            Err(_) => Err(JevConfigurationError::SecureStorage),
        }
    }
    fn delete(&self, account: &str) -> Result<(), JevConfigurationError> {
        match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, account) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(_) => Err(JevConfigurationError::SecureStorage),
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl SecretVault for KeychainVault {
    fn set(&self, _: &str, _: &str) -> Result<(), JevConfigurationError> {
        Err(JevConfigurationError::SecureStorage)
    }
    fn get(&self, _: &str) -> Result<Option<String>, JevConfigurationError> {
        Err(JevConfigurationError::SecureStorage)
    }
    fn delete(&self, _: &str) -> Result<(), JevConfigurationError> {
        Err(JevConfigurationError::SecureStorage)
    }
}

/// Local SQLite credential store, same namespace style as agent-provider credentials.
#[derive(Clone)]
pub(crate) struct SqliteVault {
    store: SettingsStore,
}

impl SqliteVault {
    pub(crate) fn new(store: SettingsStore) -> Self {
        Self { store }
    }

    fn key(account: &str) -> String {
        format!("credential.jev.{account}")
    }
}

impl SecretVault for SqliteVault {
    fn set(&self, account: &str, secret: &str) -> Result<(), JevConfigurationError> {
        self.store
            .set_string(&Self::key(account), secret)
            .map_err(|_| JevConfigurationError::SecureStorage)
    }
    fn get(&self, account: &str) -> Result<Option<String>, JevConfigurationError> {
        self.store
            .get_string(&Self::key(account))
            .map_err(|_| JevConfigurationError::SecureStorage)
    }
    fn delete(&self, account: &str) -> Result<(), JevConfigurationError> {
        self.store
            .delete_string(&Self::key(account))
            .map_err(|_| JevConfigurationError::SecureStorage)
    }
}

/// No Debug or serialization: secret values only cross the secure-store boundary in memory.
#[derive(Clone)]
pub(crate) struct JevSettings {
    store: SettingsStore,
}

impl JevSettings {
    pub(crate) fn new(store: SettingsStore) -> Self {
        Self { store }
    }

    pub(crate) fn store(&self) -> &SettingsStore {
        &self.store
    }

    pub(crate) fn production_enabled(&self) -> bool {
        false
    }

    /// Copies a pre-SQLite Keychain credential into `vault`, then drops the Keychain entry.
    /// Best effort: any failure leaves the legacy entry intact. Blocking IO; background only.
    pub(crate) fn migrate_legacy_key(&self, vault: &dyn SecretVault, legacy: &dyn SecretVault) {
        let Ok(_operation) = CONFIGURATION_LOCK.lock() else {
            return;
        };
        let Ok(Some(account)) = self.store.get_string(JEV_CREDENTIAL_REF) else {
            return;
        };
        if !matches!(vault.get(&account), Ok(None)) {
            return;
        }
        let Ok(Some(secret)) = legacy.get(&account) else {
            return;
        };
        if vault.set(&account, &secret).is_ok() {
            let _ = legacy.delete(&account);
        }
    }

    /// Blocking IO; call only on the background executor.
    pub(crate) fn load_key(
        &self,
        vault: &dyn SecretVault,
    ) -> Result<Option<String>, JevConfigurationError> {
        let _operation = CONFIGURATION_LOCK
            .lock()
            .map_err(|_| JevConfigurationError::SettingsStorage)?;
        // Retirement is journaled before switching the active reference and can be
        // retried without making a successfully saved credential unusable.
        let _ = self.cleanup_retired(vault);
        let reference = self
            .store
            .get_string(JEV_CREDENTIAL_REF)
            .map_err(|_| JevConfigurationError::SettingsStorage)?;
        reference
            .map(|account| vault.get(&account))
            .transpose()
            .map(Option::flatten)
    }

    pub(crate) fn save_key(
        &self,
        key: &str,
        vault: &dyn SecretVault,
    ) -> Result<(), JevConfigurationError> {
        let _operation = CONFIGURATION_LOCK
            .lock()
            .map_err(|_| JevConfigurationError::SettingsStorage)?;
        validate_key(key)?;
        let previous = self
            .store
            .get_string(JEV_CREDENTIAL_REF)
            .map_err(|_| JevConfigurationError::SettingsStorage)?;
        let mut retired = self.retired_refs()?;
        if let Some(previous) = previous {
            if !retired.contains(&previous) {
                retired.push(previous);
            }
        }
        let account = format!("jev-{}", uuid::Uuid::new_v4());
        vault.set(&account, key.trim())?;
        let cleanup = (!retired.is_empty())
            .then(|| serde_json::to_string(&retired))
            .transpose()
            .map_err(|_| JevConfigurationError::SettingsStorage)?;
        if self
            .store
            .apply_string_changes(&[
                (JEV_CREDENTIAL_REF.into(), Some(account.clone())),
                (PENDING_CLEANUP.into(), cleanup),
            ])
            .is_err()
        {
            let _ = vault.delete(&account);
            return Err(JevConfigurationError::SettingsStorage);
        }
        let _ = self.cleanup_retired(vault);
        Ok(())
    }

    pub(crate) fn clear(&self, vault: &dyn SecretVault) -> Result<(), JevConfigurationError> {
        let _operation = CONFIGURATION_LOCK
            .lock()
            .map_err(|_| JevConfigurationError::SettingsStorage)?;
        self.cleanup_retired(vault)?;
        if let Some(account) = self
            .store
            .get_string(JEV_CREDENTIAL_REF)
            .map_err(|_| JevConfigurationError::SettingsStorage)?
        {
            let previous = vault.get(&account)?;
            vault.delete(&account)?;
            if self.store.delete_string(JEV_CREDENTIAL_REF).is_err() {
                if let Some(secret) = previous {
                    vault.set(&account, &secret)?;
                }
                return Err(JevConfigurationError::SettingsStorage);
            }
            return Ok(());
        }
        self.store
            .delete_string(JEV_CREDENTIAL_REF)
            .map_err(|_| JevConfigurationError::SettingsStorage)
    }

    fn retired_refs(&self) -> Result<Vec<String>, JevConfigurationError> {
        self.store
            .get_string(PENDING_CLEANUP)
            .map_err(|_| JevConfigurationError::SettingsStorage)?
            .map(|value| {
                serde_json::from_str(&value).map_err(|_| JevConfigurationError::SettingsStorage)
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }

    fn cleanup_retired(&self, vault: &dyn SecretVault) -> Result<(), JevConfigurationError> {
        let retired = self.retired_refs()?;
        if retired.is_empty() {
            return Ok(());
        }
        let active = self
            .store
            .get_string(JEV_CREDENTIAL_REF)
            .map_err(|_| JevConfigurationError::SettingsStorage)?;
        let remaining: Vec<_> = retired
            .into_iter()
            .filter(|account| {
                active.as_deref() == Some(account.as_str()) || vault.delete(account).is_err()
            })
            .collect();
        let value = (!remaining.is_empty())
            .then(|| serde_json::to_string(&remaining))
            .transpose()
            .map_err(|_| JevConfigurationError::SettingsStorage)?;
        self.store
            .apply_string_changes(&[(PENDING_CLEANUP.into(), value)])
            .map_err(|_| JevConfigurationError::SettingsStorage)?;
        if remaining.is_empty() {
            Ok(())
        } else {
            Err(JevConfigurationError::SecureStorage)
        }
    }
}

fn validate_key(key: &str) -> Result<(), JevConfigurationError> {
    if key.trim().is_empty() || key.len() > 16_384 || key.bytes().any(|b| b.is_ascii_control()) {
        return Err(JevConfigurationError::InvalidKey);
    }
    Ok(())
}

pub(crate) struct JevVerification {
    pub model: String,
    pub elapsed_ms: u128,
}

/// Only a fixed, synthetic Choice request. Never receives voice text or canvas context.
pub(crate) fn verify_key(key: String) -> Result<JevVerification, JevConfigurationError> {
    validate_key(&key)?;
    let request = JevChoiceRequest::new(
        "Configuration connectivity test. The user authorizes PLAN_0: acknowledge this synthetic test. No task may be executed.".into(),
        BTreeMap::from([("PLAN_0".into(), "Acknowledge the synthetic configuration test".into()),
            ("CLARIFY".into(), "Ask when the synthetic test is ambiguous".into())]),
    ).map_err(|_| JevConfigurationError::InvalidResponse)?;
    let client = JevChoiceClient::new(key.trim().to_owned()).map_err(map_error)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| JevConfigurationError::Network)?;
    let start = Instant::now();
    let observation = runtime
        .block_on(client.observe(
            &request,
            Duration::from_secs(15),
            tokio_util::sync::CancellationToken::new(),
        ))
        .map_err(map_error)?;
    Ok(JevVerification {
        model: observation.model,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

fn map_error(error: JevError) -> JevConfigurationError {
    match error {
        JevError::Http { status } => JevConfigurationError::Http(status),
        JevError::Timeout => JevConfigurationError::Timeout,
        JevError::Network | JevError::Cancelled => JevConfigurationError::Network,
        JevError::InvalidRequest => JevConfigurationError::InvalidKey,
        JevError::InvalidResponse => JevConfigurationError::InvalidResponse,
    }
}
