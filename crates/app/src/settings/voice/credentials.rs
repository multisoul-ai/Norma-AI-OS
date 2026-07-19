#[cfg(all(not(any(test, feature = "e2e-test-support")), target_os = "macos"))]
const KEYCHAIN_SERVICE: &str = "com.norma-ai.os.agent-provider";
#[cfg(all(not(any(test, feature = "e2e-test-support")), target_os = "macos"))]
const KEYCHAIN_ITEM_NOT_FOUND: i32 = -25_300;

pub(super) fn new_account(provider: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_ACCOUNT: AtomicU64 = AtomicU64::new(1);
    let sequence = NEXT_ACCOUNT.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{provider}-{}-{nanos}-{sequence}", std::process::id())
}

#[cfg(any(test, feature = "e2e-test-support"))]
use std::collections::HashMap;
#[cfg(any(test, feature = "e2e-test-support"))]
use std::sync::{Mutex, OnceLock};

#[cfg(any(test, feature = "e2e-test-support"))]
fn memory_credentials() -> &'static Mutex<HashMap<String, String>> {
    static CREDENTIALS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CREDENTIALS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(any(test, feature = "e2e-test-support"))]
pub(super) fn store(account: &str, secret: &str) -> Result<(), String> {
    memory_credentials()
        .lock()
        .map_err(|_| "test credential store lock was poisoned".to_string())?
        .insert(account.to_string(), secret.to_string());
    Ok(())
}

#[cfg(any(test, feature = "e2e-test-support"))]
pub(super) fn load(account: &str) -> Result<Option<String>, String> {
    memory_credentials()
        .lock()
        .map_err(|_| "test credential store lock was poisoned".to_string())
        .map(|credentials| credentials.get(account).cloned())
}

#[cfg(any(test, feature = "e2e-test-support"))]
pub(super) fn delete(account: &str) -> Result<(), String> {
    memory_credentials()
        .lock()
        .map_err(|_| "test credential store lock was poisoned".to_string())?
        .remove(account);
    Ok(())
}

#[cfg(all(not(any(test, feature = "e2e-test-support")), target_os = "macos"))]
pub(super) fn store(account: &str, secret: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password(
        KEYCHAIN_SERVICE,
        account,
        secret.as_bytes(),
    )
    .map_err(|error| format!("could not save provider credential in Keychain: {error}"))
}

#[cfg(all(not(any(test, feature = "e2e-test-support")), target_os = "macos"))]
pub(super) fn load(account: &str) -> Result<Option<String>, String> {
    match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, account) {
        Ok(secret) => String::from_utf8(secret)
            .map(Some)
            .map_err(|_| "provider credential in Keychain is not valid UTF-8".to_string()),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(format!(
            "could not read provider credential from Keychain: {error}"
        )),
    }
}

#[cfg(all(not(any(test, feature = "e2e-test-support")), target_os = "macos"))]
pub(super) fn delete(account: &str) -> Result<(), String> {
    match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, account) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(format!(
            "could not delete provider credential from Keychain: {error}"
        )),
    }
}

#[cfg(all(not(any(test, feature = "e2e-test-support")), not(target_os = "macos")))]
pub(super) fn store(_account: &str, _secret: &str) -> Result<(), String> {
    Err("provider credentials require macOS Keychain".to_string())
}

#[cfg(all(not(any(test, feature = "e2e-test-support")), not(target_os = "macos")))]
pub(super) fn load(_account: &str) -> Result<Option<String>, String> {
    Err("provider credentials require macOS Keychain".to_string())
}

#[cfg(all(not(any(test, feature = "e2e-test-support")), not(target_os = "macos")))]
pub(super) fn delete(_account: &str) -> Result<(), String> {
    Err("provider credentials require macOS Keychain".to_string())
}
