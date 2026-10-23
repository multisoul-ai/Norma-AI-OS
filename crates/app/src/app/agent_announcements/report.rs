//! 大脑下发的播报文字：沿用既有的播报队列、TTS 与豆包通道。

use super::{
    AgentAnnouncement, AgentAnnouncementTtsAvailability, MultisoulApp, PendingAgentAnnouncement,
    MAX_PENDING_ANNOUNCEMENTS,
};
use gpui::Context;
use norma::protocol::ReportPriority;

impl AgentAnnouncement {
    pub(super) fn report(text: String) -> Self {
        Self {
            node_id: 0,
            text,
            agent_name: "Norma".into(),
            outcome_word: "Report".into(),
            report: true,
        }
    }
}

impl MultisoulApp {
    /// 返回是否进入了播报队列。关闭了“Agent 播报”或 TTS 不可用时丢弃。
    pub(crate) fn speak_brain_report(
        &mut self,
        text: String,
        priority: ReportPriority,
        cx: &mut Context<Self>,
    ) -> bool {
        let text = text.trim().to_string();
        if text.is_empty()
            || !self
                .voice_settings
                .values()
                .agent_task_announcements_enabled()
        {
            tracing::info!("大脑播报被跳过：文字为空或语音汇报未开启");
            return false;
        }
        let availability = self.agent_announcement_tts_availability();
        if availability == AgentAnnouncementTtsAvailability::Unavailable {
            tracing::info!("大脑播报被跳过：TTS 不可用");
            return false;
        }
        let pending = PendingAgentAnnouncement {
            announcement: AgentAnnouncement::report(text),
            waiting_for_tts: availability == AgentAnnouncementTtsAvailability::Waiting,
            deferred: availability == AgentAnnouncementTtsAvailability::Waiting,
            queued_at: std::time::Instant::now(),
        };
        let queue = &mut self.agent_announcements.pending;
        if queue.len() == MAX_PENDING_ANNOUNCEMENTS {
            queue.pop_front();
        }
        match priority {
            ReportPriority::Now => queue.push_front(pending),
            ReportPriority::Normal => queue.push_back(pending),
        }
        tracing::info!(?priority, pending_count = queue.len(), "大脑播报已入队");
        self.start_next_agent_announcement(cx);
        true
    }
}
