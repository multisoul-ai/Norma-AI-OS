//! Scope cancellation is synchronous; dispatcher cleanup may follow asynchronously.
use norma::protocol::BrainScope;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(super) struct Scopes {
    stop: Arc<AtomicBool>,
    tokens: Arc<Mutex<HashMap<BrainScope, Arc<AtomicBool>>>>,
}

impl Scopes {
    pub(super) fn new(stop: Arc<AtomicBool>) -> Self {
        Self {
            stop,
            tokens: Arc::default(),
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

    pub(super) fn release(&self, scope: &BrainScope) {
        // Keep a tombstone: a delayed producer cannot revive a released route.
        // Reopening a project receives a fresh route ID from the App endpoint.
        self.token(scope).store(true, Ordering::Relaxed);
    }

    pub(super) fn stop_all(&self) {
        self.stop.store(true, Ordering::Relaxed);
        for token in self.tokens.lock().expect("scope lock").values() {
            token.store(true, Ordering::Relaxed);
        }
    }
}
