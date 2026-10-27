use super::AgentAnnouncement;
use crate::app::doubao::playback::PlaybackQueue;
use crate::app::voice_window_inset::{TtsPlaybackEvent, VoiceWindowInsetEventSink};
use canvas::NodeId;
use std::sync::Arc;

/// Foreground ownership is distinct from the background monitor's queue handle.
/// Releasing the App must stop audio even while the monitor is still awaiting callbacks.
pub(super) struct AgentAnnouncementAudioControl {
    pub(super) queue: Arc<PlaybackQueue>,
}
impl AgentAnnouncementAudioControl {
    pub(super) fn cancel(&self) {
        self.queue.end();
    }
}
impl Drop for AgentAnnouncementAudioControl {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(super) enum AgentAnnouncementPlaybackPhase {
    Idle,
    Reporting {
        announcement: AgentAnnouncement,
        cancellation: tokio_util::sync::CancellationToken,
        /// 最早入队时间：被打断后按原时效重新排队。
        queued_at: std::time::Instant,
    },
    Synthesizing(AgentAnnouncement),
    Playing {
        announcement: AgentAnnouncement,
        started: bool,
    },
}

pub(crate) struct AgentAnnouncementPlayback {
    pub(crate) generation: u64,
    /// Bumped only by explicit stop/mode/mute cancellation, never by normal playback.
    pub(super) delivery_epoch: u64,
    pub(super) phase: AgentAnnouncementPlaybackPhase,
    pub(super) control: Option<AgentAnnouncementAudioControl>,
    stopping: Option<Arc<PlaybackQueue>>,
    pub(super) playback_events: Vec<TtsPlaybackEvent>,
    playback_event_sink: Option<VoiceWindowInsetEventSink>,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) e2e_enabled: bool,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) e2e_tts_ready: bool,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) e2e_spoken: Vec<String>,
}

impl Default for AgentAnnouncementPlayback {
    fn default() -> Self {
        Self {
            generation: 0,
            delivery_epoch: 0,
            phase: AgentAnnouncementPlaybackPhase::Idle,
            control: None,
            stopping: None,
            playback_events: Vec::new(),
            playback_event_sink: None,
            #[cfg(feature = "e2e-test-support")]
            e2e_enabled: false,
            #[cfg(feature = "e2e-test-support")]
            e2e_tts_ready: false,
            #[cfg(feature = "e2e-test-support")]
            e2e_spoken: Vec::new(),
        }
    }
}

impl AgentAnnouncementPlayback {
    pub(crate) fn connect_playback_event_sink(&mut self, sink: VoiceWindowInsetEventSink) {
        self.playback_event_sink = Some(sink);
    }

    pub(super) fn push_playback_event(&mut self, event: TtsPlaybackEvent) {
        if let Some(sink) = &self.playback_event_sink {
            sink.push(event);
        } else {
            self.playback_events.push(event);
        }
    }

    pub(super) fn is_idle(&self) -> bool {
        matches!(self.phase, AgentAnnouncementPlaybackPhase::Idle)
            && self
                .stopping
                .as_ref()
                .is_none_or(|queue| queue.is_stopped())
    }

    pub(super) fn active_node_id(&self) -> Option<NodeId> {
        match &self.phase {
            AgentAnnouncementPlaybackPhase::Idle => None,
            AgentAnnouncementPlaybackPhase::Synthesizing(announcement)
            | AgentAnnouncementPlaybackPhase::Reporting { announcement, .. }
            | AgentAnnouncementPlaybackPhase::Playing { announcement, .. } => {
                Some(announcement.node_id)
            }
        }
    }

    pub(super) fn cancel_active(&mut self) {
        match &self.phase {
            AgentAnnouncementPlaybackPhase::Synthesizing(announcement)
            | AgentAnnouncementPlaybackPhase::Playing { announcement, .. } => {
                if self.control.is_none() {
                    if let Some(delivery) = &announcement.delivery {
                        delivery.cancel();
                    }
                }
            }
            // Once handed to Doubao, only its audio worker can classify cancellation.
            AgentAnnouncementPlaybackPhase::Reporting { .. }
            | AgentAnnouncementPlaybackPhase::Idle => {}
        }
        self.generation = self.generation.wrapping_add(1);
        let had_active = !matches!(self.phase, AgentAnnouncementPlaybackPhase::Idle);
        let started = matches!(
            self.phase,
            AgentAnnouncementPlaybackPhase::Playing { started: true, .. }
        );
        if let Some(control) = self.control.take() {
            control.cancel();
            self.stopping = Some(Arc::clone(&control.queue));
        }
        if let AgentAnnouncementPlaybackPhase::Reporting { cancellation, .. } = &self.phase {
            cancellation.cancel();
        }
        self.playback_events.retain(|event| {
            !matches!(
                event,
                TtsPlaybackEvent::PcmQueued(_) | TtsPlaybackEvent::Started
            )
        });
        if started {
            self.push_playback_event(TtsPlaybackEvent::Interrupted);
        } else if had_active {
            self.push_playback_event(TtsPlaybackEvent::DiscardQueued);
        }
        self.phase = AgentAnnouncementPlaybackPhase::Idle;
    }
}
