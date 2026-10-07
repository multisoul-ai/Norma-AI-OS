//! Agent task terminal announcement policy and playback coordination.

#[cfg(feature = "e2e-test-support")]
use super::voice_window_inset::PcmEnergyTimeline;
use super::voice_window_inset::TtsPlaybackEvent;
use super::MultisoulApp;
use canvas::{CanvasNodeKind, NodeId};
use gpui::Context;
use runtime::{AgentTaskState, RuntimeEvent};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::SystemTime;

mod availability;
mod cache;
mod dispatch;
mod doubao;
mod playback;
mod queue;
mod synthesis;
pub(super) use cache::AgentAnnouncementAudioCache;
pub(crate) use playback::AgentAnnouncementPlayback;
use playback::AgentAnnouncementPlaybackPhase;

const MAX_PENDING_ANNOUNCEMENTS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AnnouncementEligibility {
    enabled: bool,
    tts_availability: AgentAnnouncementTtsAvailability,
}

impl AnnouncementEligibility {
    #[cfg(test)]
    pub(super) fn new(enabled: bool, tts_available: bool) -> Self {
        Self {
            enabled,
            tts_availability: if tts_available {
                AgentAnnouncementTtsAvailability::Ready
            } else {
                AgentAnnouncementTtsAvailability::Unavailable
            },
        }
    }

    #[cfg(test)]
    fn waiting_for_tts(enabled: bool) -> Self {
        Self {
            enabled,
            tts_availability: AgentAnnouncementTtsAvailability::Waiting,
        }
    }

    fn with_tts_availability(
        enabled: bool,
        tts_availability: AgentAnnouncementTtsAvailability,
    ) -> Self {
        Self {
            enabled,
            tts_availability,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AgentAnnouncementTtsAvailability {
    Ready,
    Waiting,
    Unavailable,
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
    Cancelled,
    Interrupted,
}

impl AgentAnnouncementOutcome {
    fn from_task_event(to: &AgentTaskState, source_event: &str) -> Option<Self> {
        match (to, source_event) {
            (AgentTaskState::Done, "Stop" | "turn/completed") => Some(Self::Done),
            (AgentTaskState::Failed { .. }, "StopFailure" | "turn/completed") => Some(Self::Failed),
            _ => None,
        }
    }

    fn spoken_word(self) -> &'static str {
        match self {
            Self::Done => "Done",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
            Self::Interrupted => "Interrupted",
        }
    }
}

#[derive(Debug, Default)]
struct AgentTaskRound {
    terminal_claimed: bool,
}

#[derive(Debug)]
struct PendingAgentAnnouncement {
    announcement: AgentAnnouncement,
    waiting_for_tts: bool,
}

#[derive(Debug, Default)]
pub(super) struct AgentAnnouncements {
    rounds: HashMap<NodeId, AgentTaskRound>,
    pending: VecDeque<PendingAgentAnnouncement>,
    reported_tasks: HashSet<u64>,
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
                .retain(|pending| pending.announcement.node_id != *node_id);
            return;
        }
        let (node_id, outcome, trusted_terminal_event) = match event {
            RuntimeEvent::AgentTaskStateChanged {
                node_id,
                source_event,
                ..
            } if matches!(source_event.as_str(), "UserPromptSubmit" | "turn/started") => {
                self.rounds.insert(*node_id, AgentTaskRound::default());
                tracing::debug!(node_id, "agent announcement task round opened");
                return;
            }
            RuntimeEvent::AgentTaskStateChanged {
                node_id,
                to: AgentTaskState::Ended | AgentTaskState::Ready,
                ..
            } => {
                let terminal_claimed = self
                    .rounds
                    .get(node_id)
                    .is_some_and(|round| round.terminal_claimed);
                if !terminal_claimed {
                    self.rounds.remove(node_id);
                }
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
                (*node_id, outcome, true)
            }
            RuntimeEvent::NodeRunStateChanged {
                node_id,
                to: runtime::NodeRunState::Error { .. },
                ..
            } => (*node_id, AgentAnnouncementOutcome::Failed, false),
            _ => return,
        };
        if !self.rounds.contains_key(&node_id) && trusted_terminal_event {
            tracing::warn!(
                node_id,
                outcome = outcome.spoken_word(),
                "agent announcement recovered task round from terminal event"
            );
            self.rounds.insert(node_id, AgentTaskRound::default());
        }
        let Some(round) = self.rounds.get_mut(&node_id) else {
            tracing::debug!(
                node_id,
                outcome = outcome.spoken_word(),
                "agent announcement terminal ignored because no active task round exists"
            );
            return;
        };
        if round.terminal_claimed {
            tracing::debug!(
                node_id,
                outcome = outcome.spoken_word(),
                "agent announcement duplicate terminal ignored"
            );
            return;
        }
        round.terminal_claimed = true;
        self.enqueue_terminal(node_id, agent_name, outcome, eligibility);
    }

    fn take_next_ready(&mut self) -> Option<AgentAnnouncement> {
        if self.pending.front()?.waiting_for_tts {
            return None;
        }
        self.pending.pop_front().map(|pending| pending.announcement)
    }

    fn clear_pending(&mut self) -> usize {
        let cleared = self.pending.len();
        self.pending.clear();
        cleared
    }

    fn reset(&mut self) {
        self.rounds.clear();
        self.pending.clear();
        self.reported_tasks.clear();
    }
}

impl MultisoulApp {
    pub(super) fn handle_agent_runtime_events(
        &mut self,
        events: &[RuntimeEvent],
        cx: &mut Context<Self>,
    ) {
        let tts_availability = self.agent_announcement_tts_availability();
        let eligibility = AnnouncementEligibility::with_tts_availability(
            self.voice_settings
                .values()
                .agent_task_announcements_enabled(),
            tts_availability,
        );
        let doubao = self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Doubao;
        let tasks = self
            .agent_task_coordinator
            .as_ref()
            .map(|coordinator| coordinator.tasks())
            .unwrap_or_default();
        for event in events {
            let node_id = runtime_event_node_id(event);
            if doubao
                && matches!(
                    event,
                    RuntimeEvent::AgentTaskStateChanged {
                        to: AgentTaskState::Done
                            | AgentTaskState::Failed { .. }
                            | AgentTaskState::Ended,
                        ..
                    }
                )
                && self
                    .agent_announcements
                    .handle_coordinated_terminal(event, &tasks, eligibility)
            {
                continue;
            }
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
        self.agent_announcements
            .queue_coordinated_results(&tasks, events, eligibility, doubao);
        self.start_next_agent_announcement(cx);
    }

    pub(crate) fn cancel_agent_announcements(&mut self) {
        let active_node_id = self.agent_announcement_playback.active_node_id();
        let cleared_pending = self.agent_announcements.clear_pending();
        if active_node_id.is_some() || cleared_pending > 0 {
            tracing::info!(
                ?active_node_id,
                cleared_pending,
                "agent announcements cancelled"
            );
        }
        self.agent_announcement_playback.cancel_active();
    }

    pub(crate) fn reset_agent_announcements(&mut self) {
        self.agent_announcements.reset();
        self.agent_announcement_playback.cancel_active();
    }

    pub(crate) fn clear_agent_announcement_audio_for_voice_change(&mut self) {
        self.agent_announcement_audio_cache.clear_for_voice_change();
    }

    pub(crate) fn prune_agent_announcement_audio_cache(&mut self) {
        self.agent_announcement_audio_cache
            .prune_expired_workspaces(SystemTime::now());
    }

    pub(crate) fn agent_announcement_audio_idle(&self) -> bool {
        self.agent_announcement_playback.is_idle()
    }

    fn mark_agent_announcement_started(&mut self, generation: u64) -> bool {
        if generation != self.agent_announcement_playback.generation {
            return false;
        }
        let started_announcement = match &mut self.agent_announcement_playback.phase {
            AgentAnnouncementPlaybackPhase::Playing {
                announcement,
                started,
            } if !*started => {
                *started = true;
                Some((announcement.node_id, announcement.outcome_word.clone()))
            }
            _ => None,
        };
        let Some((node_id, outcome)) = started_announcement else {
            return false;
        };
        self.agent_announcement_playback
            .push_playback_event(TtsPlaybackEvent::Started);
        tracing::info!(
            node_id,
            outcome = %outcome,
            "agent announcement playback started"
        );
        true
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
            &phase,
            AgentAnnouncementPlaybackPhase::Playing { started: true, .. }
        ) {
            self.agent_announcement_playback
                .push_playback_event(if completed {
                    TtsPlaybackEvent::Finished
                } else {
                    TtsPlaybackEvent::Interrupted
                });
        }
        if let AgentAnnouncementPlaybackPhase::Playing { announcement, .. } = &phase {
            tracing::info!(
                node_id = announcement.node_id,
                outcome = %announcement.outcome_word,
                completed,
                "agent announcement playback finished"
            );
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
            .push_playback_event(TtsPlaybackEvent::PcmQueued(PcmEnergyTimeline::from_pcm(
                &chunk,
            )));
        self.agent_announcement_playback
            .push_playback_event(TtsPlaybackEvent::Started);
        self.agent_announcement_playback.phase = AgentAnnouncementPlaybackPhase::Playing {
            announcement,
            started: true,
        };
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent_announcements_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/doubao_announcements_tests.rs"]
mod doubao_tests;

#[cfg(test)]
#[path = "../../tests/unit/doubao_task_results_tests.rs"]
mod task_results_tests;
