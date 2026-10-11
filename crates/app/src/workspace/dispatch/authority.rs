//! Revocable memory leases. Permission mutations close leases before their database commit.
use norma::workspace_dispatch::WorkspaceCardRef;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
};

struct Lease {
    database: PathBuf,
    card: WorkspaceCardRef,
    authority: Weak<AtomicBool>,
}
#[derive(Default)]
struct Leases {
    active: Vec<Lease>,
    disabled: HashSet<(PathBuf, WorkspaceCardRef)>,
    mutations: HashMap<(PathBuf, WorkspaceCardRef), Weak<Mutex<()>>>,
    removing: HashMap<RemovalScope, usize>,
    removed: HashSet<RemovalScope>,
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct RemovalScope {
    database: PathBuf,
    workspace: String,
    area: Option<String>,
}
impl RemovalScope {
    fn contains(&self, database: &Path, card: &WorkspaceCardRef) -> bool {
        self.database == database
            && self.workspace == card.workspace_id
            && self.area.as_ref().is_none_or(|area| *area == card.area_id)
    }
}

/// A cancelled deletion conservatively leaves the scope closed. Only a known failed
/// transaction permits new attempts; completing a deletion never revives old input.
pub(crate) struct ScopeRemoval(RemovalScope);
pub(crate) fn begin_scope_removal(
    database: &Path,
    workspace: &str,
    area: Option<&str>,
) -> ScopeRemoval {
    let scope = RemovalScope {
        database: database.to_owned(),
        workspace: workspace.into(),
        area: area.map(str::to_owned),
    };
    if let Ok(mut state) = leases().lock() {
        *state.removing.entry(scope.clone()).or_default() += 1;
        for lease in &state.active {
            if scope.contains(&lease.database, &lease.card) {
                if let Some(authority) = lease.authority.upgrade() {
                    authority.store(false, Ordering::Release);
                }
            }
        }
    }
    ScopeRemoval(scope)
}
impl ScopeRemoval {
    pub(crate) fn finish(self, success: bool) {
        if let Ok(mut state) = leases().lock() {
            if success {
                state.removed.insert(self.0.clone());
            }
            if let Some(count) = state.removing.get_mut(&self.0) {
                *count -= 1;
                if *count == 0 {
                    state.removing.remove(&self.0);
                }
            }
        }
    }
}
fn leases() -> &'static Mutex<Leases> {
    static LEASES: OnceLock<Mutex<Leases>> = OnceLock::new();
    LEASES.get_or_init(|| Mutex::new(Leases::default()))
}

pub(crate) fn register(database: &Path, card: &WorkspaceCardRef) -> Arc<AtomicBool> {
    let authority = Arc::new(AtomicBool::new(false));
    if let Ok(mut state) = leases().lock() {
        state
            .active
            .retain(|lease| lease.authority.strong_count() > 0);
        authority.store(
            !state
                .removing
                .keys()
                .chain(&state.removed)
                .any(|scope| scope.contains(database, card))
                && !state
                    .disabled
                    .contains(&(database.to_owned(), card.clone())),
            Ordering::Release,
        );
        state.active.push(Lease {
            database: database.to_owned(),
            card: card.clone(),
            authority: Arc::downgrade(&authority),
        });
    }
    authority
}

/// Call off the foreground. A failed save permits new attempts but never revives old input leases.
pub(crate) fn set_role_enabled(
    database: &Path,
    card: &WorkspaceCardRef,
    enabled: bool,
) -> storage::StorageResult<storage::WorkspaceAgentRole> {
    // Only this card's background mutations wait for SQLite. Foreground register/revoke
    // never acquire this lock, and the registry lock is released before database IO.
    let mutation = {
        let mut state = leases()
            .lock()
            .map_err(|_| std::io::Error::other("workspace authority unavailable"))?;
        state.mutations.retain(|_, lock| lock.strong_count() > 0);
        let key = (database.to_owned(), card.clone());
        let lock = state
            .mutations
            .get(&key)
            .and_then(Weak::upgrade)
            .unwrap_or_else(|| Arc::new(Mutex::new(())));
        state.mutations.insert(key, Arc::downgrade(&lock));
        lock
    };
    let _serial = mutation
        .lock()
        .map_err(|_| std::io::Error::other("workspace role mutation unavailable"))?;
    if !enabled {
        let mut state = leases()
            .lock()
            .map_err(|_| std::io::Error::other("workspace authority unavailable"))?;
        state.disabled.insert((database.to_owned(), card.clone()));
        revoke(&mut state, database, Some(card), None);
    }
    let result =
        storage::WorkspaceAgentRoleStore::new(database.to_owned()).set_enabled(card, enabled);
    if enabled && result.is_ok() || !enabled && result.is_err() {
        if let Ok(mut state) = leases().lock() {
            state.disabled.remove(&(database.to_owned(), card.clone()));
        }
    }
    result
}

pub(crate) fn revoke_area(database: &Path, workspace: &str, area: &str) {
    if let Ok(mut state) = leases().lock() {
        revoke(&mut state, database, None, Some((workspace, area)));
    }
}
pub(crate) fn revoke_card(database: &Path, card: &WorkspaceCardRef) {
    if let Ok(mut state) = leases().lock() {
        revoke(&mut state, database, Some(card), None);
    }
}
fn revoke(
    state: &mut Leases,
    database: &Path,
    card: Option<&WorkspaceCardRef>,
    area: Option<(&str, &str)>,
) {
    state.active.retain(|lease| {
        let Some(authority) = lease.authority.upgrade() else {
            return false;
        };
        if lease.database == database
            && card.is_none_or(|card| lease.card == *card)
            && area.is_none_or(|(workspace, area)| {
                lease.card.workspace_id == workspace && lease.card.area_id == area
            })
        {
            authority.store(false, Ordering::Release);
        }
        true
    });
}

#[cfg(test)]
#[path = "../../../tests/unit/workspace_dispatch_authority_tests.rs"]
mod tests;
