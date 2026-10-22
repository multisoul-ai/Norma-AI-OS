//! 排队通知的时效与合并：通道重连期间积压的通知过期即丢弃，放行时积压部分合成一句。
use super::{AgentAnnouncement, AgentAnnouncements};
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "../../../tests/unit/agent_announcement_freshness_tests.rs"]
mod tests;

/// 超过 60 秒的完成通知已失去即时意义；画布卡片状态仍保留结果。
pub(super) const ANNOUNCEMENT_TTL: Duration = Duration::from_secs(60);
const MERGED_SEPARATOR: &str = "; ";

impl AgentAnnouncements {
    pub(super) fn discard_expired(&mut self, now: Instant) -> usize {
        let before = self.pending.len();
        self.pending
            .retain(|pending| now.saturating_duration_since(pending.queued_at) < ANNOUNCEMENT_TTL);
        let discarded = before - self.pending.len();
        if discarded > 0 {
            tracing::info!(
                discarded,
                ttl_secs = ANNOUNCEMENT_TTL.as_secs(),
                "expired agent announcements discarded"
            );
        }
        discarded
    }

    /// 取出下一条就绪通知。队首若是积压通知，则连同其后连续就绪的积压通知合成一句，
    /// 同一卡片只留最新一条；非积压通知仍逐条播报。
    /// 返回值带最早的入队时间，供播报被打断后按原时效重新排队。
    pub(super) fn take_ready_batch(
        &mut self,
        now: Instant,
    ) -> Option<(AgentAnnouncement, Instant)> {
        self.discard_expired(now);
        if self.pending.front()?.waiting_for_tts {
            return None;
        }
        let first = self.pending.pop_front()?;
        let merge_backlog = first.deferred;
        let mut batch = vec![first];
        while merge_backlog
            && self
                .pending
                .front()
                .is_some_and(|pending| pending.deferred && !pending.waiting_for_tts)
        {
            let Some(next) = self.pending.pop_front() else {
                break;
            };
            batch.retain(|pending| pending.announcement.node_id != next.announcement.node_id);
            batch.push(next);
        }
        let queued_at = batch.iter().map(|pending| pending.queued_at).min()?;
        let merged_count = batch.len();
        let mut announcements = batch.into_iter().map(|pending| pending.announcement);
        let mut merged = announcements.next()?;
        for next in announcements {
            merged.text.push_str(MERGED_SEPARATOR);
            merged.text.push_str(&next.text);
        }
        if merged_count > 1 {
            tracing::info!(merged_count, "agent announcements merged into one report");
        }
        Some((merged, queued_at))
    }
}
