//! Scope cancellation is synchronous; dispatcher cleanup may follow asynchronously.
use norma::protocol::BrainScope;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

type TurnKey = (BrainScope, String);
type LiveTurns = Mutex<HashMap<TurnKey, (u64, Arc<AtomicBool>)>>;

#[derive(Clone)]
pub(crate) struct Scopes {
    stop: Arc<AtomicBool>,
    tokens: Arc<Mutex<HashMap<BrainScope, Arc<AtomicBool>>>>,
    turns: Arc<LiveTurns>,
}

pub(crate) struct StaffTurn<'a> {
    scopes: &'a Scopes,
    key: TurnKey,
    pub(crate) cancelled: Arc<AtomicBool>,
}

impl Drop for StaffTurn<'_> {
    fn drop(&mut self) {
        self.scopes
            .turns
            .lock()
            .expect("staff turn lock")
            .remove(&self.key);
    }
}

impl Scopes {
    pub(super) fn new(stop: Arc<AtomicBool>) -> Self {
        Self {
            stop,
            tokens: Arc::default(),
            turns: Arc::default(),
        }
    }

    pub(super) fn token(&self, scope: &BrainScope) -> Arc<AtomicBool> {
        self.tokens
            .lock()
            .expect("scope lock")
            .entry(scope.clone())
            .or_insert_with(|| Arc::new(AtomicBool::new(self.stop.load(Ordering::Relaxed))))
            .clone()
    }

    /// The route token remains valid for queued jobs. Only the active staff turn gets its own flag.
    pub(crate) fn staff_turn(
        &self,
        scope: &BrainScope,
        task_id: &str,
        round: u64,
        route_cancelled: &AtomicBool,
    ) -> StaffTurn<'_> {
        let key = (scope.clone(), task_id.to_owned());
        let mut turns = self.turns.lock().expect("staff turn lock");
        let cancelled = Arc::new(AtomicBool::new(route_cancelled.load(Ordering::Relaxed)));
        turns.insert(key.clone(), (round, cancelled.clone()));
        StaffTurn {
            scopes: self,
            key,
            cancelled,
        }
    }

    pub(super) fn stop_staff_turn(&self, scope: &BrainScope, task_id: &str, round: u64) -> bool {
        self.turns
            .lock()
            .expect("staff turn lock")
            .get(&(scope.clone(), task_id.to_owned()))
            .is_some_and(|(active_round, cancelled)| {
                *active_round == round && !cancelled.swap(true, Ordering::Relaxed)
            })
    }

    pub(super) fn release(&self, scope: &BrainScope) {
        // Keep a tombstone: a delayed producer cannot revive a released route.
        // Reopening a project receives a fresh route ID from the App endpoint.
        self.token(scope).store(true, Ordering::Relaxed);
        for ((owner, _), (_, cancelled)) in self.turns.lock().expect("staff turn lock").iter() {
            if owner == scope {
                cancelled.store(true, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn stop_all(&self) {
        self.stop.store(true, Ordering::Relaxed);
        for token in self.tokens.lock().expect("scope lock").values() {
            token.store(true, Ordering::Relaxed);
        }
        for (_, cancelled) in self.turns.lock().expect("staff turn lock").values() {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}
