use super::AgentAnnouncement;
use crate::app::dialogue::{create_assistant_audio_sink, TtsClientBox};
use crate::app::voice::playback::AssistantAudioSink;
use crate::app::voice_window_inset::TtsPlaybackEvent;
use canvas::NodeId;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub(super) struct AgentAnnouncementAudioControl {
    cancelled: AtomicBool,
    sink: Mutex<Option<Box<dyn AssistantAudioSink>>>,
}

impl AgentAnnouncementAudioControl {
    pub(super) fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            sink: Mutex::new(None),
        }
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut current) = self.sink.try_lock() {
            if let Some(mut sink) = current.take() {
                sink.stop();
                let _ = std::thread::Builder::new()
                    .name("agent_announcement_audio_release".to_string())
                    .spawn(move || drop(sink));
            }
        }
    }

    pub(super) fn play_with_completion(
        &self,
        chunk: &audio::PcmAudioChunk,
    ) -> Result<(async_channel::Receiver<()>, async_channel::Receiver<()>), String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("Agent announcement was cancelled".to_string());
        }
        let mut sink = create_assistant_audio_sink();
        let (started_tx, started_rx) = async_channel::bounded(1);
        let (finished_tx, finished_rx) = async_channel::bounded(1);
        sink.write_chunk_with_completion(chunk, started_tx, finished_tx)?;
        if self.cancelled.load(Ordering::Acquire) {
            sink.stop();
            return Err("Agent announcement was cancelled".to_string());
        }
        let mut current = self
            .sink
            .lock()
            .map_err(|_| "Agent announcement sink lock was poisoned".to_string())?;
        if self.cancelled.load(Ordering::Acquire) {
            sink.stop();
            return Err("Agent announcement was cancelled".to_string());
        }
        *current = Some(sink);
        Ok((started_rx, finished_rx))
    }
}

pub(super) enum AgentAnnouncementPlaybackPhase {
    Idle,
    Synthesizing(AgentAnnouncement),
    Playing {
        announcement: AgentAnnouncement,
        started: bool,
    },
}

pub(crate) struct AgentAnnouncementPlayback {
    pub(crate) generation: u64,
    pub(super) phase: AgentAnnouncementPlaybackPhase,
    pub(super) control: Option<Arc<AgentAnnouncementAudioControl>>,
    pub(super) tts_client: Option<TtsClientBox>,
    pub(super) playback_events: Vec<TtsPlaybackEvent>,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) e2e_enabled: bool,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) e2e_spoken: Vec<String>,
}

impl Default for AgentAnnouncementPlayback {
    fn default() -> Self {
        Self {
            generation: 0,
            phase: AgentAnnouncementPlaybackPhase::Idle,
            control: None,
            tts_client: None,
            playback_events: Vec::new(),
            #[cfg(feature = "e2e-test-support")]
            e2e_enabled: false,
            #[cfg(feature = "e2e-test-support")]
            e2e_spoken: Vec::new(),
        }
    }
}

impl AgentAnnouncementPlayback {
    pub(super) fn is_idle(&self) -> bool {
        matches!(self.phase, AgentAnnouncementPlaybackPhase::Idle)
    }

    pub(super) fn active_node_id(&self) -> Option<NodeId> {
        match &self.phase {
            AgentAnnouncementPlaybackPhase::Idle => None,
            AgentAnnouncementPlaybackPhase::Synthesizing(announcement)
            | AgentAnnouncementPlaybackPhase::Playing { announcement, .. } => {
                Some(announcement.node_id)
            }
        }
    }

    pub(super) fn cancel_active(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        let had_active = !matches!(self.phase, AgentAnnouncementPlaybackPhase::Idle);
        let started = matches!(
            self.phase,
            AgentAnnouncementPlaybackPhase::Playing { started: true, .. }
        );
        if let Some(control) = self.control.take() {
            control.cancel();
        }
        self.playback_events.retain(|event| {
            !matches!(
                event,
                TtsPlaybackEvent::PcmQueued(_) | TtsPlaybackEvent::Started
            )
        });
        if started {
            self.playback_events.push(TtsPlaybackEvent::Interrupted);
        } else if had_active {
            self.playback_events.push(TtsPlaybackEvent::DiscardQueued);
        }
        self.phase = AgentAnnouncementPlaybackPhase::Idle;
    }

    pub(crate) fn take_playback_events(&mut self) -> Vec<TtsPlaybackEvent> {
        std::mem::take(&mut self.playback_events)
    }

    pub(super) fn take_tts_client_for_configuration_change(&mut self) -> Option<TtsClientBox> {
        self.tts_client.take()
    }
}
