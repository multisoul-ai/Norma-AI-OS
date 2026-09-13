use crate::app::MultisoulApp;

impl MultisoulApp {
    pub(crate) fn stop_project(&mut self, cx: &mut gpui::Context<Self>) {
        crate::workspace::sessions::remove(cx.entity_id(), cx);
        self.unregister_workspace_window_if_needed(cx);
        self.workspace_id = None;
        self.workspace_route_id = None;
        self.workspace_state.current_root = None;
        self.project_window = None;
        self.background_project = false;
        self.background_restore = None;
        self.pending_agent_submissions.clear();
        let _ = self.session_reply_queue.interrupt_all();
        self.agent_task_coordinator = None;
        self.agent_runtime = None;
        self.dialogue.on_window_closed();
        self.voice.cancel_speech_task();
        self.voice.cancel_speech_session();
        self.flush_persistence_now();
        self.runtime.kill_all();
    }
}

impl Drop for MultisoulApp {
    fn drop(&mut self) {
        self.dialogue.on_window_closed();
        self.voice.cancel_speech_task();
        self.voice.cancel_speech_session();
        self.flush_persistence_now();
        self.runtime.kill_all();
    }
}
