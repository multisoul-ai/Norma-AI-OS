//! 持续采集线程的取消与回收边界。

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(super) struct EgoWorkerGuard {
    cancel: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl EgoWorkerGuard {
    pub(super) fn new(cancel: Arc<AtomicBool>, handle: std::thread::JoinHandle<()>) -> Self {
        Self {
            cancel,
            handle: Some(handle),
        }
    }

    #[cfg(feature = "l3-test-support")]
    pub(super) fn shutdown(mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for EgoWorkerGuard {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = std::thread::Builder::new()
                .name("norma-ego-worker-reaper".to_string())
                .spawn(move || {
                    let _ = handle.join();
                });
        }
    }
}
