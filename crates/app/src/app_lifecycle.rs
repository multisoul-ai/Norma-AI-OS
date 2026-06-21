use crate::app::MultisoulApp;

impl Drop for MultisoulApp {
    fn drop(&mut self) {
        self.flush_persistence_now();
        self.runtime.kill_all();
    }
}
