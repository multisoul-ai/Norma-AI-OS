use super::MultisoulApp;
use crate::app::voice_window_inset::TtsPlaybackSource;
use gpui::Context;
use runtime::RuntimeEvent;

impl MultisoulApp {
    pub fn enable_agent_announcement_test_audio_for_e2e(&mut self) {
        self.agent_announcement_playback.e2e_enabled = true;
    }

    pub fn inject_runtime_event_for_e2e(&mut self, event: RuntimeEvent, cx: &mut Context<Self>) {
        self.handle_agent_runtime_events(&[event], cx);
    }

    pub fn spoken_agent_announcements_for_e2e(&self) -> Vec<String> {
        self.agent_announcement_playback.e2e_spoken.clone()
    }

    pub fn finish_agent_announcement_for_e2e(&mut self, cx: &mut Context<Self>) {
        let generation = self.agent_announcement_playback.generation;
        self.finish_agent_announcement_playback(generation, true, cx);
    }

    pub fn flush_tts_playback_events_for_e2e(&mut self, cx: &mut Context<Self>) {
        for event in self.dialogue.take_playback_events() {
            self.voice_window_inset
                .apply_playback_event(TtsPlaybackSource::Assistant, event);
        }
        for event in self.agent_announcement_playback.take_playback_events() {
            self.voice_window_inset
                .apply_playback_event(TtsPlaybackSource::AgentAnnouncement, event);
        }
        for event in self.tts_preview.take_playback_events() {
            self.voice_window_inset
                .apply_playback_event(TtsPlaybackSource::Preview, event);
        }
        cx.notify();
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

    pub fn finish_tts_preview_for_e2e(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.tts_preview.finish_playback(generation);
        self.flush_tts_playback_events_for_e2e(cx);
    }
}
