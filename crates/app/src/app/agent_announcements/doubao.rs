use super::{
    AgentAnnouncementPlaybackPhase, MultisoulApp, PendingAgentAnnouncement,
    MAX_PENDING_ANNOUNCEMENTS,
};
use crate::app::doubao::DoubaoReport;
use gpui::Context;

impl MultisoulApp {
    pub(super) fn start_next_doubao_agent_announcement(&mut self, cx: &mut Context<Self>) {
        if !self.agent_announcement_playback.is_idle()
            || !self.refresh_agent_announcement_tts_availability()
        {
            return;
        }
        let Some(sender) = self.doubao.reports.as_ref() else {
            return;
        };
        let Some((announcement, queued_at)) = self
            .agent_announcements
            .take_ready_batch(std::time::Instant::now())
        else {
            return;
        };
        let (mut report, completed) = DoubaoReport::new(announcement.text.clone());
        report.expires_at = Some(queued_at + super::freshness::ANNOUNCEMENT_TTL);
        self.agent_announcement_playback.generation =
            self.agent_announcement_playback.generation.wrapping_add(1);
        let generation = self.agent_announcement_playback.generation;
        self.agent_announcement_playback.phase = AgentAnnouncementPlaybackPhase::Reporting {
            announcement,
            cancellation: report.cancellation.clone(),
            queued_at,
        };
        let _ = sender.try_send(report);
        cx.spawn(async move |this, cx| {
            let completed = completed.await.is_ok();
            let _ = this.update(cx, |app, cx| {
                if generation != app.agent_announcement_playback.generation {
                    return;
                }
                let phase = std::mem::replace(
                    &mut app.agent_announcement_playback.phase,
                    AgentAnnouncementPlaybackPhase::Idle,
                );
                let AgentAnnouncementPlaybackPhase::Reporting {
                    announcement,
                    queued_at,
                    ..
                } = phase
                else {
                    return;
                };
                tracing::info!(
                    node_id = announcement.node_id,
                    completed,
                    "Doubao agent announcement playback finished"
                );
                // 过期而未播的通知不再回队，直接轮到下一条。
                let fresh = queued_at.elapsed() < super::freshness::ANNOUNCEMENT_TTL;
                if !completed && fresh {
                    // Pending capacity stays 32: keep this older interrupted report plus at most 31 later reports.
                    app.agent_announcements
                        .pending
                        .truncate(MAX_PENDING_ANNOUNCEMENTS - 1);
                    app.agent_announcements
                        .pending
                        .push_front(PendingAgentAnnouncement {
                            announcement,
                            waiting_for_tts: true,
                            deferred: true,
                            queued_at,
                        });
                } else {
                    app.start_next_agent_announcement(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}
