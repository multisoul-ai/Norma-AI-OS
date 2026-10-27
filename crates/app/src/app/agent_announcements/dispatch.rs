use super::cache::CachedAnnouncementSegments;
use super::playback::AgentAnnouncementPlaybackPhase;
use super::synthesis::{
    assemble_cached_agent_terminal_announcement,
    synthesize_agent_terminal_announcement_with_cached_segments, synthesize_report_text,
    SynthesizedAgentTerminalAnnouncement,
};
use crate::app::dialogue::create_tts_client_for_selection;
use crate::app::MultisoulApp;
use dialogue::DialoguePhase;
use gpui::Context;
use std::sync::Arc;
use std::time::SystemTime;

impl MultisoulApp {
    pub(crate) fn start_next_agent_announcement(&mut self, cx: &mut Context<Self>) {
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::GptLive
        {
            return;
        }
        // 关闭“智能体任务播报”只停固定词播报；大脑的汇报由标题栏的“语音汇报”开关管。
        if !self
            .voice_settings
            .values()
            .agent_task_announcements_enabled()
            && !self.brain_speaking()
        {
            self.cancel_agent_announcements();
            return;
        }
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Doubao
        {
            self.start_next_doubao_agent_announcement(cx);
            return;
        }
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
        let announcement_is_report = announcement.report;
        let report_text = announcement.text.clone();
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
        let selected = self.dialogue.selected_tts_model.clone();
        let selected_for_create = selected.clone();
        self.agent_announcement_playback.phase =
            AgentAnnouncementPlaybackPhase::Synthesizing(announcement);
        let report_text = announcement_is_report.then_some(report_text);
        let work = cx.background_executor().spawn(async move {
            if let Some(text) = report_text {
                return tts_runtime.with_client(
                    selected,
                    || create_tts_client_for_selection(selected_for_create.as_ref()),
                    |tts| synthesize_report_text(tts, &text),
                );
            }
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
                if let Some(delivery) = &announcement.delivery {
                    delivery.set(agent::context::ReportDelivery::Failed);
                }
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
        if workspace_matches_current && !announcement.report {
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
        self.start_legacy_announcement_playback(
            generation,
            announcement,
            synthesized.pcm,
            crate::app::dialogue::create_assistant_audio_sink,
            cx,
        );
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/agent_announcement_settings_sync_tests.rs"]
mod settings_sync_tests;
