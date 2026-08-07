use crate::app::MultisoulApp;

impl Drop for MultisoulApp {
    fn drop(&mut self) {
        self.dialogue.on_window_closed();
        self.voice.cancel_speech_task();
        self.voice.cancel_speech_session();
        self.flush_persistence_now();
        self.runtime.kill_all();
    }
}
