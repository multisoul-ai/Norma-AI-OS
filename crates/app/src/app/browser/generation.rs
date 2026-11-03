//! A native view owns one event generation; queued deliveries never own its lifetime.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Weak,
};

pub(crate) struct GenerationOwner(Arc<AtomicBool>);
impl GenerationOwner {
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
    pub(crate) fn lease(&self) -> Generation {
        Generation(Arc::downgrade(&self.0))
    }
    pub(crate) fn invalidate(&self) {
        self.lease().invalidate();
    }
}
impl Drop for GenerationOwner {
    fn drop(&mut self) {
        self.invalidate();
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Generation(Weak<AtomicBool>);
impl PartialEq for Generation {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}
impl Generation {
    pub(crate) fn is_current(&self) -> bool {
        self.0
            .upgrade()
            .is_some_and(|live| live.load(Ordering::Acquire))
    }
    pub(crate) fn invalidate(&self) {
        if let Some(live) = self.0.upgrade() {
            live.store(false, Ordering::Release);
        }
    }
    pub(crate) fn queue<T>(&self, event: T) -> Queued<T> {
        Queued {
            generation: self.clone(),
            event,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Queued<T> {
    generation: Generation,
    event: T,
}
impl<T> Queued<T> {
    pub(crate) fn consume(self) -> Option<T> {
        self.generation.is_current().then_some(self.event)
    }
}
