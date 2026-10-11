use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

// Paths enter this snapshot only after background SHA-256 validation. Render
// paths never scan or hash the cache; the OS app process owns its lifetime.
static VERIFIED: OnceLock<RwLock<HashMap<String, PathBuf>>> = OnceLock::new();

pub(super) fn publish(rel: &str, path: PathBuf) {
    if let Ok(mut entries) = VERIFIED.get_or_init(Default::default).write() {
        entries.insert(rel.into(), path);
    }
}

pub(crate) fn resolve(rel: &str) -> Option<PathBuf> {
    VERIFIED.get()?.read().ok()?.get(rel).cloned()
}

pub(super) fn revoke(rel: &str) {
    if let Some(entries) = VERIFIED.get() {
        if let Ok(mut entries) = entries.write() {
            entries.remove(rel);
        }
    }
}
