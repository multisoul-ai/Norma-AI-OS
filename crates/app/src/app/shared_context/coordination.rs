use super::*;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
};

type Recovery = Arc<dyn Fn(&ContextAccess) -> StorageResult<()> + Send + Sync>;
pub(super) struct Coordination {
    pub(super) serial: tokio::sync::Mutex<Option<Recovery>>,
    pub(super) gate: Mutex<ContextExecutionGate>,
    pub(super) pending: AtomicBool,
    // Accessed only under serial; all consumers of this scope share initialization.
    pub(super) initialized: AtomicBool,
}
type Registry = Mutex<HashMap<(PathBuf, String), Arc<Coordination>>>;

/// DatabasePaths supplies the same absolute path to all consumers. No filesystem IO here.
pub(super) fn for_scope(
    path: PathBuf,
    scope: ContextScope,
) -> Result<Arc<Coordination>, AdapterError> {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    if !path.is_absolute() {
        return Err(AdapterError::Rejected(
            "context_database_requires_absolute_path".into(),
        ));
    }
    let key = (path, serde_json::to_string(&scope).map_err(failed)?);
    let mut registry = REGISTRY
        .get_or_init(Mutex::default)
        .lock()
        .map_err(failed)?;
    // Accepted pending writes belong to the process host, including periods with
    // no canvas/model consumer. Only settled, unused coordinators may be retired.
    registry
        .retain(|_, state| Arc::strong_count(state) > 1 || state.pending.load(Ordering::Acquire));
    if let Some(state) = registry.get(&key) {
        return Ok(state.clone());
    }
    let state = Arc::new(Coordination {
        serial: tokio::sync::Mutex::new(None),
        gate: Mutex::new(ContextExecutionGate::new(scope)),
        pending: AtomicBool::new(false),
        initialized: AtomicBool::new(false),
    });
    registry.insert(key, state.clone());
    Ok(state)
}
