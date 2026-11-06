//! Live project grants shared by every ACP lane; updating them performs no IO.
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

#[derive(Clone, Debug, Default)]
pub struct ReadRoots(Arc<RwLock<Vec<PathBuf>>>);

impl ReadRoots {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self(Arc::new(RwLock::new(roots)))
    }

    /// Replace the entire set, including an empty set when the last project closes.
    pub fn replace(&self, roots: Vec<PathBuf>) {
        *self.0.write().expect("brain read roots lock") = roots;
    }

    pub(super) fn snapshot(&self) -> Vec<PathBuf> {
        self.0.read().expect("brain read roots lock").clone()
    }
}
