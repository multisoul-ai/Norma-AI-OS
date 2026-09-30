use storage::SettingsStore;

const CREDENTIAL_PREFIX: &str = "credential.agent-provider.";

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

fn key(account: &str) -> String {
    format!("{CREDENTIAL_PREFIX}{account}")
}

pub(super) fn store(store: &SettingsStore, account: &str, secret: &str) -> Result<(), String> {
    store
        .set_string(&key(account), secret)
        .map_err(|error| format!("could not save provider credential in SQLite: {error}"))
}

pub(super) fn load(store: &SettingsStore, account: &str) -> Result<Option<String>, String> {
    store
        .get_string(&key(account))
        .map_err(|error| format!("could not read provider credential from SQLite: {error}"))
}

pub(super) fn delete(store: &SettingsStore, account: &str) -> Result<(), String> {
    store
        .delete_string(&key(account))
        .map_err(|error| format!("could not delete provider credential from SQLite: {error}"))
}
