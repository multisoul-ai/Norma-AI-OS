use super::MultisoulApp;
use gpui::Context;
use runtime::RuntimeEvent;

impl MultisoulApp {
    pub fn enable_agent_announcement_test_audio_for_e2e(&mut self) {
        self.agent_announcement_playback.e2e_enabled = true;
        self.agent_announcement_playback.e2e_tts_ready = true;
    }

    pub fn set_agent_announcement_test_tts_ready_for_e2e(
        &mut self,
        ready: bool,
        cx: &mut Context<Self>,
    ) {
        self.agent_announcement_playback.e2e_enabled = true;
        self.agent_announcement_playback.e2e_tts_ready = ready;
        self.start_next_agent_announcement(cx);
    }

    pub fn inject_runtime_event_for_e2e(&mut self, event: RuntimeEvent, cx: &mut Context<Self>) {
        self.handle_agent_runtime_events(&[event], cx);
    }

    pub fn spoken_agent_announcements_for_e2e(&self) -> Vec<String> {
        self.agent_announcement_playback.e2e_spoken.clone()
    }

    pub fn agent_announcement_active_for_e2e(&self) -> bool {
        !self.agent_announcement_audio_idle()
    }

    pub fn unclassified_speech_start_for_e2e(&mut self) -> bool {
        let (_, should_notify) = self.handle_unclassified_speech_start();
        should_notify
    }

    pub fn queue_dialogue_agent_report_for_e2e(&mut self, report: impl Into<String>) {
        self.dialogue.queue_agent_report(report.into());
    }

    pub fn queue_single_voice_dialogue_agent_report_for_e2e(&mut self, report: impl Into<String>) {
        self.dialogue.queue_single_voice_agent_report(report.into());
    }

    pub fn begin_next_dialogue_agent_report_for_e2e(&mut self) -> Option<String> {
        self.dialogue.begin_next_agent_report()
    }

    pub fn finish_agent_announcement_for_e2e(&mut self, cx: &mut Context<Self>) {
        let generation = self.agent_announcement_playback.generation;
        self.finish_agent_announcement_playback(generation, true, cx);
    }

    pub fn flush_tts_playback_events_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.voice_window_inset_view.update(cx, |view, voice_cx| {
            view.advance_for_e2e(std::time::Duration::ZERO, voice_cx)
        });
    }

    pub fn begin_tts_preview_pcm_for_e2e(
        &mut self,
        chunk: audio::PcmAudioChunk,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.cancel_agent_announcements();
        let (generation, control) = self.tts_preview.start();
        self.dialogue.track_tts_preview(control);
        self.tts_preview.queue_pcm(generation, &chunk);
        self.tts_preview.mark_playback_started(generation);
        self.flush_tts_playback_events_for_e2e(cx);
        generation
    }

    pub fn queue_pending_tts_preview_for_e2e(
        &mut self,
        chunk: audio::PcmAudioChunk,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.cancel_agent_announcements();
        let (generation, control) = self.tts_preview.start();
        self.dialogue.track_tts_preview(control);
        self.tts_preview.queue_pcm(generation, &chunk);
        self.tts_preview.mark_playback_started(generation);
        let _ = cx;
        generation
    }

    pub fn pending_tts_playback_requests_next_frame_for_e2e(&self) -> bool {
        self.voice_window_inset_events.is_animation_active()
    }

    pub fn finish_tts_preview_for_e2e(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.tts_preview.finish_playback(generation);
        self.flush_tts_playback_events_for_e2e(cx);
    }

    pub fn tts_preview_pcm_queue_count_for_e2e(&self) -> u64 {
        self.tts_preview.pcm_queue_count()
    }
}
