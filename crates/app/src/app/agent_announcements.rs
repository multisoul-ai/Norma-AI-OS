//! Agent task terminal announcement policy and playback coordination.

use super::dialogue::{create_tts_client_for_selection, selected_tts_model};
use super::voice_window_inset::{PcmEnergyTimeline, TtsPlaybackEvent};
use super::MultisoulApp;
use canvas::{CanvasNodeKind, NodeId};
use dialogue::DialoguePhase;
use gpui::Context;
use runtime::{AgentTaskState, RuntimeEvent};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

mod playback;
mod synthesis;
pub(crate) use playback::AgentAnnouncementPlayback;
use playback::{
    await_playback_completion_or_timeout, completion_timeout_for_chunk,
    AgentAnnouncementAudioControl, AgentAnnouncementPlaybackPhase,
};
use synthesis::synthesize_agent_terminal_announcement;

const MAX_PENDING_ANNOUNCEMENTS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AnnouncementEligibility {
    enabled: bool,
    tts_available: bool,
}

impl AnnouncementEligibility {
    pub(super) fn new(enabled: bool, tts_available: bool) -> Self {
        Self {
            enabled,
            tts_available,
        }
    }

    fn allows_announcement(self) -> bool {
        self.enabled && self.tts_available
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AgentAnnouncement {
    node_id: NodeId,
    text: String,
    agent_name: String,
    outcome_word: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentAnnouncementOutcome {
    Done,
    Failed,
}

impl AgentAnnouncementOutcome {
    fn from_task_event(to: &AgentTaskState, source_event: &str) -> Option<Self> {
        match (to, source_event) {
            (AgentTaskState::Done, "Stop") => Some(Self::Done),
            (AgentTaskState::Failed { .. }, "StopFailure") => Some(Self::Failed),
            _ => None,
        }
    }

    fn spoken_word(self) -> &'static str {
        match self {
            Self::Done => "Done",
            Self::Failed => "Failed",
        }
    }
}

#[derive(Debug, Default)]
struct AgentTaskRound {
    terminal_claimed: bool,
}

#[derive(Debug, Default)]
pub(super) struct AgentAnnouncements {
    rounds: HashMap<NodeId, AgentTaskRound>,
    pending: VecDeque<AgentAnnouncement>,
}

impl AgentAnnouncements {
    pub(super) fn handle_runtime_event(
        &mut self,
        event: &RuntimeEvent,
        agent_name: Option<&str>,
        eligibility: AnnouncementEligibility,
    ) {
        if let RuntimeEvent::NodeRemoved { node_id, .. } = event {
            self.rounds.remove(node_id);
            self.pending
                .retain(|announcement| announcement.node_id != *node_id);
            return;
        }
        let (node_id, outcome) = match event {
            RuntimeEvent::AgentTaskStateChanged {
                node_id,
                source_event,
                ..
            } if source_event == "UserPromptSubmit" => {
                self.rounds.insert(*node_id, AgentTaskRound::default());
                return;
            }
            RuntimeEvent::AgentTaskStateChanged {
                node_id,
                to: AgentTaskState::Ended | AgentTaskState::Ready,
                ..
            } => {
                self.rounds.remove(node_id);
                return;
            }
            RuntimeEvent::AgentTaskStateChanged {
                node_id,
                to,
                source_event,
                ..
            } => {
                let Some(outcome) = AgentAnnouncementOutcome::from_task_event(to, source_event)
                else {
                    return;
                };
                (*node_id, outcome)
            }
            RuntimeEvent::NodeRunStateChanged {
                node_id,
                to: runtime::NodeRunState::Error { .. },
                ..
            } => (*node_id, AgentAnnouncementOutcome::Failed),
            _ => return,
        };
        let Some(round) = self.rounds.get_mut(&node_id) else {
            return;
        };
        if round.terminal_claimed {
            return;
        }
        round.terminal_claimed = true;
        if !eligibility.allows_announcement() {
            return;
        }
        let Some(agent_name) = agent_name.map(str::trim).filter(|name| !name.is_empty()) else {
            return;
        };
        if self.pending.len() == MAX_PENDING_ANNOUNCEMENTS {
            let dropped = self.pending.pop_front();
            tracing::warn!(
                dropped_node_id = ?dropped.map(|announcement| announcement.node_id),
                "agent announcement queue is full; dropped the oldest pending announcement"
            );
        }
        self.pending
            .push_back(AgentAnnouncement::terminal(node_id, agent_name, outcome));
    }

    fn take_next_ready(&mut self) -> Option<AgentAnnouncement> {
        self.pending.pop_front()
    }

    fn clear_pending(&mut self) {
        self.pending.clear();
    }

    fn reset(&mut self) {
        self.rounds.clear();
        self.pending.clear();
    }
}

impl MultisoulApp {
    fn agent_announcement_playback_e2e_enabled(&self) -> bool {
        #[cfg(feature = "e2e-test-support")]
        {
            self.agent_announcement_playback.e2e_enabled
        }
        #[cfg(not(feature = "e2e-test-support"))]
        false
    }

    pub(super) fn handle_agent_runtime_events(
        &mut self,
        events: &[RuntimeEvent],
        cx: &mut Context<Self>,
    ) {
        let eligibility = AnnouncementEligibility::new(
            self.voice_settings
                .values()
                .agent_task_announcements_enabled(),
            super::dialogue::selected_tts_model_matches(
                self.voice_settings.values().active_tts_model_id(),
                self.voice_settings.values().active_tts_voice_id(),
            ) || cfg!(feature = "e2e-test-support")
                && self.agent_announcement_playback_e2e_enabled(),
        );
        for event in events {
            let node_id = runtime_event_node_id(event);
            let agent_name = node_id.and_then(|node_id| self.agent_announcement_name(node_id));
            self.agent_announcements.handle_runtime_event(
                event,
                agent_name.as_deref(),
                eligibility,
            );
            if matches!(event, RuntimeEvent::NodeRemoved { .. })
                && node_id == self.agent_announcement_playback.active_node_id()
            {
                self.agent_announcement_playback.cancel_active();
            }
        }
        self.start_next_agent_announcement(cx);
    }

    pub(crate) fn cancel_agent_announcements(&mut self) {
        self.agent_announcements.clear_pending();
        self.agent_announcement_playback.cancel_active();
    }

    pub(crate) fn reset_agent_announcements(&mut self) {
        self.agent_announcements.reset();
        self.agent_announcement_playback.cancel_active();
    }

    pub(crate) fn agent_announcement_audio_idle(&self) -> bool {
        self.agent_announcement_playback.is_idle()
    }

    pub(crate) fn start_next_agent_announcement(&mut self, cx: &mut Context<Self>) {
        if !self.agent_announcement_playback.is_idle()
            || matches!(
                self.dialogue.phase(),
                DialoguePhase::Thinking | DialoguePhase::Speaking
            )
            || self.dialogue.has_pending_spoken_reply()
            || self.tts_preview.is_active()
        {
            return;
        }
        let Some(announcement) = self.agent_announcements.take_next_ready() else {
            return;
        };
        #[cfg(feature = "e2e-test-support")]
        if self.agent_announcement_playback.e2e_enabled {
            self.start_agent_announcement_for_e2e(announcement);
            return;
        }
        self.agent_announcement_playback.generation =
            self.agent_announcement_playback.generation.wrapping_add(1);
        let generation = self.agent_announcement_playback.generation;
        let agent_name = announcement.agent_name.clone();
        let outcome_word = announcement.outcome_word.clone();
        let tts_runtime = self.dialogue.shared_tts_runtime.clone();
        let selected = selected_tts_model();
        let selected_for_create = selected.clone();
        self.agent_announcement_playback.phase =
            AgentAnnouncementPlaybackPhase::Synthesizing(announcement);
        let work = cx.background_executor().spawn(async move {
            tts_runtime.with_client(
                selected,
                || create_tts_client_for_selection(selected_for_create.as_ref()),
                |tts| synthesize_agent_terminal_announcement(tts, &agent_name, &outcome_word),
            )
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.finish_agent_announcement_synthesis(generation, result, cx);
            });
        })
        .detach();
    }

    fn finish_agent_announcement_synthesis(
        &mut self,
        generation: u64,
        result: Result<audio::PcmAudioChunk, voice_sherpa::SpeechError>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.agent_announcement_playback.generation {
            cx.background_executor()
                .spawn(async move { drop(result) })
                .detach();
            return;
        }
        let AgentAnnouncementPlaybackPhase::Synthesizing(announcement) = std::mem::replace(
            &mut self.agent_announcement_playback.phase,
            AgentAnnouncementPlaybackPhase::Idle,
        ) else {
            return;
        };
        let chunk = match result {
            Ok(chunk) => chunk,
            Err(error) => {
                tracing::warn!(%error, "agent announcement synthesis failed");
                self.start_next_agent_announcement(cx);
                return;
            }
        };
        let control = Arc::new(AgentAnnouncementAudioControl::new());
        self.agent_announcement_playback.control = Some(Arc::clone(&control));
        self.agent_announcement_playback.phase =
            AgentAnnouncementPlaybackPhase::Synthesizing(announcement.clone());
        let playback_work = cx.background_executor().spawn(async move {
            let playback = control.play_with_completion(&chunk);
            (control, chunk, playback)
        });
        cx.spawn(async move |this, cx| {
            let (control, chunk, playback) = playback_work.await;
            let _ = this.update(cx, |app, cx| {
                app.finish_agent_announcement_scheduling(
                    generation,
                    announcement,
                    control,
                    chunk,
                    playback,
                    cx,
                );
            });
        })
        .detach();
    }

    fn finish_agent_announcement_scheduling(
        &mut self,
        generation: u64,
        announcement: AgentAnnouncement,
        control: Arc<AgentAnnouncementAudioControl>,
        chunk: audio::PcmAudioChunk,
        playback: Result<(async_channel::Receiver<()>, async_channel::Receiver<()>), String>,
        cx: &mut Context<Self>,
    ) {
        if generation != self.agent_announcement_playback.generation {
            control.cancel();
            return;
        }
        let (started_rx, finished_rx) = match playback {
            Ok(receivers) => receivers,
            Err(error) => {
                tracing::warn!(%error, "agent announcement playback failed");
                self.agent_announcement_playback.control = None;
                self.agent_announcement_playback.phase = AgentAnnouncementPlaybackPhase::Idle;
                self.start_next_agent_announcement(cx);
                return;
            }
        };
        self.agent_announcement_playback
            .playback_events
            .push(TtsPlaybackEvent::PcmQueued(PcmEnergyTimeline::from_pcm(
                &chunk,
            )));
        self.agent_announcement_playback.phase = AgentAnnouncementPlaybackPhase::Playing {
            announcement,
            started: false,
        };
        cx.notify();
        let completion_timeout = completion_timeout_for_chunk(&chunk);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            if started_rx.recv().await.is_ok() {
                let _ = this.update(cx, |app, cx| {
                    app.mark_agent_announcement_started(generation);
                    cx.notify();
                });
            }
            let completed =
                await_playback_completion_or_timeout(executor, finished_rx, completion_timeout)
                    .await;
            if !completed {
                tracing::warn!(
                    generation,
                    watchdog_ms = completion_timeout.as_millis(),
                    "agent announcement playback ended without a completion callback"
                );
            }
            let _ = this.update(cx, |app, cx| {
                app.finish_agent_announcement_playback(generation, completed, cx);
            });
        })
        .detach();
    }

    fn mark_agent_announcement_started(&mut self, generation: u64) {
        if generation != self.agent_announcement_playback.generation {
            return;
        }
        let AgentAnnouncementPlaybackPhase::Playing { started, .. } =
            &mut self.agent_announcement_playback.phase
        else {
            return;
        };
        if !*started {
            *started = true;
            self.agent_announcement_playback
                .playback_events
                .push(TtsPlaybackEvent::Started);
        }
    }

    pub(super) fn finish_agent_announcement_playback(
        &mut self,
        generation: u64,
        completed: bool,
        cx: &mut Context<Self>,
    ) {
        if generation != self.agent_announcement_playback.generation {
            return;
        }
        let phase = std::mem::replace(
            &mut self.agent_announcement_playback.phase,
            AgentAnnouncementPlaybackPhase::Idle,
        );
        if let Some(control) = self.agent_announcement_playback.control.take() {
            cx.background_executor()
                .spawn(async move { drop(control) })
                .detach();
        }
        if matches!(
            phase,
            AgentAnnouncementPlaybackPhase::Playing { started: true, .. }
        ) {
            self.agent_announcement_playback
                .playback_events
                .push(if completed {
                    TtsPlaybackEvent::Finished
                } else {
                    TtsPlaybackEvent::Interrupted
                });
        }
        self.start_next_agent_announcement(cx);
        cx.notify();
    }

    fn agent_announcement_name(&self, node_id: NodeId) -> Option<String> {
        let CanvasNodeKind::Agent(spec) = &self.runtime.document.node(node_id)?.kind else {
            return None;
        };
        Some(
            spec.alias
                .as_deref()
                .map(str::trim)
                .filter(|alias| !alias.is_empty())
                .unwrap_or_else(|| spec.profile.display_title())
                .to_string(),
        )
    }
}

fn runtime_event_node_id(event: &RuntimeEvent) -> Option<NodeId> {
    match event {
        RuntimeEvent::NodeRegistered { node_id, .. }
        | RuntimeEvent::NodeRunStateChanged { node_id, .. }
        | RuntimeEvent::NodeRemoved { node_id, .. }
        | RuntimeEvent::AgentTaskStateChanged { node_id, .. }
        | RuntimeEvent::EgoTaskStateChanged { node_id, .. } => Some(*node_id),
        RuntimeEvent::NodeOperationFailed { node_id, .. } => *node_id,
        RuntimeEvent::EventsDropped { .. } => None,
    }
}

#[cfg(feature = "e2e-test-support")]
impl MultisoulApp {
    fn start_agent_announcement_for_e2e(&mut self, announcement: AgentAnnouncement) {
        use audio::PcmFormat;

        self.agent_announcement_playback.generation =
            self.agent_announcement_playback.generation.wrapping_add(1);
        self.agent_announcement_playback
            .e2e_spoken
            .push(announcement.text.clone());
        let bytes = std::iter::repeat_n(8_000i16.to_le_bytes(), 1_600)
            .flatten()
            .collect();
        let chunk = audio::PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, bytes)
            .expect("e2e Agent announcement PCM must be valid 16kHz mono PCM16");
        self.agent_announcement_playback
            .playback_events
            .push(TtsPlaybackEvent::PcmQueued(PcmEnergyTimeline::from_pcm(
                &chunk,
            )));
        self.agent_announcement_playback
            .playback_events
            .push(TtsPlaybackEvent::Started);
        self.agent_announcement_playback.phase = AgentAnnouncementPlaybackPhase::Playing {
            announcement,
            started: true,
        };
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent_announcements_tests.rs"]
mod tests;
