use super::{AgentAnnouncementTtsAvailability, AgentAnnouncements, MultisoulApp};
use crate::settings::voice::{VoiceRequirement, VoiceRequirementState};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AnnouncementAvailabilityUpdate {
    pub(super) released: usize,
    pub(super) discarded: usize,
}

impl AgentAnnouncements {
    pub(super) fn update_tts_availability(
        &mut self,
        availability: AgentAnnouncementTtsAvailability,
    ) -> AnnouncementAvailabilityUpdate {
        let mut update = AnnouncementAvailabilityUpdate::default();
        match availability {
            AgentAnnouncementTtsAvailability::Ready => {
                for pending in &mut self.pending {
                    if pending.waiting_for_tts {
                        pending.waiting_for_tts = false;
                        update.released += 1;
                    }
                }
            }
            AgentAnnouncementTtsAvailability::Waiting => {
                for pending in &mut self.pending {
                    pending.waiting_for_tts = true;
                    pending.deferred = true;
                }
            }
            AgentAnnouncementTtsAvailability::Unavailable => {
                update.discarded = self.pending.len();
                self.pending.clear();
            }
        }
        update
    }
}

impl MultisoulApp {
    pub(super) fn agent_announcement_tts_availability(&self) -> AgentAnnouncementTtsAvailability {
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::GptLive
        {
            return AgentAnnouncementTtsAvailability::Unavailable;
        }
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Doubao
        {
            return if self
                .doubao
                .reports
                .as_ref()
                .is_some_and(|sender| !sender.is_closed())
            {
                AgentAnnouncementTtsAvailability::Ready
            } else if self
                .voice_settings
                .values()
                .doubao_credential_ref()
                .is_some()
                && !self.doubao.is_blocked()
            {
                AgentAnnouncementTtsAvailability::Waiting
            } else {
                AgentAnnouncementTtsAvailability::Unavailable
            };
        }
        #[cfg(feature = "e2e-test-support")]
        if self.agent_announcement_playback.e2e_enabled {
            return if self.agent_announcement_playback.e2e_tts_ready {
                AgentAnnouncementTtsAvailability::Ready
            } else {
                AgentAnnouncementTtsAvailability::Waiting
            };
        }

        if self.dialogue.selected_tts_model_matches(
            self.voice_settings.values().active_tts_model_id(),
            self.voice_settings.values().active_tts_voice_id(),
        ) {
            return AgentAnnouncementTtsAvailability::Ready;
        }
        let Some(provider) = self.voice_settings.values().active_tts_provider_id() else {
            return AgentAnnouncementTtsAvailability::Unavailable;
        };
        if provider.as_str() == tts::MINIMAX_TTS_PROVIDER_ID {
            return match self.voice_settings.minimax_readiness() {
                tts::RemoteTtsReadiness::Configured
                | tts::RemoteTtsReadiness::Checking
                | tts::RemoteTtsReadiness::Ready
                | tts::RemoteTtsReadiness::TemporarilyUnavailable => {
                    AgentAnnouncementTtsAvailability::Waiting
                }
                tts::RemoteTtsReadiness::Unconfigured
                | tts::RemoteTtsReadiness::InvalidCredential
                | tts::RemoteTtsReadiness::VoiceUnavailable
                | tts::RemoteTtsReadiness::VoiceValidationRequired => {
                    AgentAnnouncementTtsAvailability::Unavailable
                }
            };
        }
        match self
            .voice_settings
            .requirement_state(VoiceRequirement::SpokenReplies)
        {
            VoiceRequirementState::Ready | VoiceRequirementState::Installing { .. } => {
                AgentAnnouncementTtsAvailability::Waiting
            }
            VoiceRequirementState::Missing | VoiceRequirementState::Failed => {
                AgentAnnouncementTtsAvailability::Unavailable
            }
        }
    }

    pub(super) fn refresh_agent_announcement_tts_availability(&mut self) -> bool {
        let availability = self.agent_announcement_tts_availability();
        let update = self
            .agent_announcements
            .update_tts_availability(availability);
        if update.released > 0 {
            tracing::info!(
                released_count = update.released,
                "agent announcements released after TTS became ready"
            );
        }
        if update.discarded > 0 {
            tracing::warn!(
                discarded_count = update.discarded,
                "deferred agent announcements discarded because TTS became unavailable"
            );
        }
        availability == AgentAnnouncementTtsAvailability::Ready
    }
}

#[cfg(all(test, feature = "e2e-test-support"))]
#[path = "../../../tests/unit/gpt_live_announcements_tests.rs"]
mod live_tests;
