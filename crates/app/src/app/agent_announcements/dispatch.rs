use super::cache::CachedAnnouncementSegments;
use super::playback::{
    await_playback_completion_or_timeout, completion_timeout_for_chunk,
    AgentAnnouncementAudioControl, AgentAnnouncementPlaybackPhase,
};
use super::synthesis::{
    assemble_cached_agent_terminal_announcement,
    synthesize_agent_terminal_announcement_with_cached_segments,
    SynthesizedAgentTerminalAnnouncement,
};
use super::AgentAnnouncement;
use crate::app::dialogue::{create_tts_client_for_selection, selected_tts_model};
use crate::app::voice_window_inset::{PcmEnergyTimeline, TtsPlaybackEvent};
use crate::app::MultisoulApp;
use dialogue::DialoguePhase;
use gpui::Context;
use std::sync::Arc;
use std::time::SystemTime;

impl MultisoulApp {
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
        if !self.refresh_agent_announcement_tts_availability() {
            return;
        }
        let Some(announcement) = self.agent_announcements.take_next_ready() else {
            return;
        };
        tracing::info!(
            node_id = announcement.node_id,
            outcome = %announcement.outcome_word,
            "agent announcement dispatch started"
        );
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
        let workspace_root = self.workspace_state.current_root.clone();
        let cached_segments = workspace_root
            .as_deref()
            .map(|root| {
                self.agent_announcement_audio_cache.lookup_segments(
                    root,
                    &agent_name,
                    &outcome_word,
                    SystemTime::now(),
                )
            })
            .unwrap_or_default();
        let tts_runtime = self.dialogue.shared_tts_runtime.clone();
        let selected = selected_tts_model();
        let selected_for_create = selected.clone();
        self.agent_announcement_playback.phase =
            AgentAnnouncementPlaybackPhase::Synthesizing(announcement);
        let work = cx.background_executor().spawn(async move {
            if cached_segments.name.is_some() && cached_segments.outcome.is_some() {
                assemble_cached_agent_terminal_announcement(cached_segments)
            } else {
                tts_runtime.with_client(
                    selected,
                    || create_tts_client_for_selection(selected_for_create.as_ref()),
                    |tts| {
                        synthesize_agent_terminal_announcement_with_cached_segments(
                            tts,
                            &agent_name,
                            &outcome_word,
                            cached_segments,
                        )
                    },
                )
            }
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                app.finish_agent_announcement_synthesis(generation, workspace_root, result, cx);
            });
        })
        .detach();
    }

    fn finish_agent_announcement_synthesis(
        &mut self,
        generation: u64,
        workspace_root: Option<std::path::PathBuf>,
        result: Result<SynthesizedAgentTerminalAnnouncement, voice_sherpa::SpeechError>,
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
        let synthesized = match result {
            Ok(synthesized) => synthesized,
            Err(error) => {
                tracing::warn!(
                    node_id = announcement.node_id,
                    outcome = %announcement.outcome_word,
                    %error,
                    "agent announcement synthesis failed"
                );
                self.start_next_agent_announcement(cx);
                return;
            }
        };
        let workspace_matches_current = workspace_root
            .as_deref()
            .is_some_and(|root| self.workspace_state.current_root.as_deref() == Some(root));
        if workspace_matches_current {
            let root = workspace_root
                .as_deref()
                .expect("matching workspace root must be present");
            self.agent_announcement_audio_cache.insert_cached_segments(
                root,
                &announcement.agent_name,
                &announcement.outcome_word,
                CachedAnnouncementSegments {
                    name: Arc::clone(&synthesized.name),
                    outcome: Arc::clone(&synthesized.outcome),
                },
                SystemTime::now(),
            );
        }
        let chunk = synthesized.pcm;
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
                tracing::warn!(
                    node_id = announcement.node_id,
                    outcome = %announcement.outcome_word,
                    %error,
                    "agent announcement playback failed"
                );
                self.agent_announcement_playback.control = None;
                self.agent_announcement_playback.phase = AgentAnnouncementPlaybackPhase::Idle;
                self.start_next_agent_announcement(cx);
                return;
            }
        };
        self.agent_announcement_playback
            .push_playback_event(TtsPlaybackEvent::PcmQueued(PcmEnergyTimeline::from_pcm(
                &chunk,
            )));
        self.agent_announcement_playback.phase = AgentAnnouncementPlaybackPhase::Playing {
            announcement,
            started: false,
        };
        let completion_timeout = completion_timeout_for_chunk(&chunk);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            if started_rx.recv().await.is_ok() {
                let _ = this.update(cx, |app, cx| {
                    if app.mark_agent_announcement_started(generation) {
                        cx.notify();
                    }
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
}
