use super::{PcmEnergyTimeline, VoiceWindowInset};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TtsPlaybackSource {
    Assistant,
    AgentAnnouncement,
    Preview,
}

#[derive(Clone, Debug)]
pub(crate) enum TtsPlaybackEvent {
    PcmQueued(PcmEnergyTimeline),
    DiscardQueued,
    Started,
    Finished,
    Interrupted,
}

impl VoiceWindowInset {
    pub(crate) fn apply_playback_event(
        &mut self,
        source: TtsPlaybackSource,
        event: TtsPlaybackEvent,
    ) {
        if self.disabled {
            return;
        }
        match event {
            TtsPlaybackEvent::PcmQueued(timeline) => {
                self.activate_source(source);
                self.queue_energy_timeline(timeline);
            }
            TtsPlaybackEvent::DiscardQueued if self.playback_source == Some(source) => {
                self.clear_energy();
                if self.pending_playbacks == 0 {
                    self.playback_source = None;
                }
            }
            TtsPlaybackEvent::Started => {
                self.activate_source(source);
                self.begin_playback();
            }
            TtsPlaybackEvent::Finished if self.playback_source == Some(source) => {
                self.finish_playback();
                if self.pending_playbacks == 0 {
                    self.playback_source = None;
                }
            }
            TtsPlaybackEvent::Interrupted if self.playback_source == Some(source) => {
                self.interrupt_playback();
                self.playback_source = None;
            }
            TtsPlaybackEvent::DiscardQueued
            | TtsPlaybackEvent::Finished
            | TtsPlaybackEvent::Interrupted => {}
        }
    }

    fn activate_source(&mut self, source: TtsPlaybackSource) {
        if self.playback_source.is_some_and(|active| active != source) {
            self.interrupt_playback();
        }
        self.playback_source = Some(source);
    }
}
