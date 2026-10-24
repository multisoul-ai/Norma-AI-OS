//! 用户离开一阵又回到窗口：通知大脑做一次“回来摘要”。

use crate::app::MultisoulApp;
use std::time::{Duration, Instant};

const DEFAULT_AWAY: Duration = Duration::from_secs(600);

/// 离开多久再回来才算“离开过”。默认 10 分钟；`NORMA_BRAIN_AWAY_SECS` 可改（1 到 86400），供验收缩短。
pub(super) fn away_threshold() -> Duration {
    parse_away_secs(std::env::var("NORMA_BRAIN_AWAY_SECS").ok().as_deref())
}

pub(super) fn parse_away_secs(value: Option<&str>) -> Duration {
    value
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|secs| (1..=86_400).contains(secs))
        .map_or(DEFAULT_AWAY, Duration::from_secs)
}

/// 记录窗口失焦的起点，回来时判断是否离开够久。
#[derive(Debug, Default)]
pub(crate) struct AwayTracker {
    left_at: Option<Instant>,
}

impl AwayTracker {
    /// 窗口活跃状态变化时调用。回来且离开不短于阈值时返回离开时长；每次离开只触发一次。
    pub(crate) fn observe(
        &mut self,
        active: bool,
        now: Instant,
        threshold: Duration,
    ) -> Option<Duration> {
        if !active {
            self.left_at.get_or_insert(now);
            return None;
        }
        let left = self.left_at.take()?;
        let away = now.saturating_duration_since(left);
        (away >= threshold).then_some(away)
    }
}

impl MultisoulApp {
    /// 渲染时发现窗口活跃状态变化。只有窗口外壳（或没有外壳的独立窗口）关心。
    pub(crate) fn note_window_activity_for_brain(&mut self, active: bool) {
        if self.desktop_host.is_some() {
            return;
        }
        if let Some(away) = self
            .brain
            .away
            .observe(active, Instant::now(), away_threshold())
        {
            self.brain.pending_return = Some(away);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_away_tests.rs"]
mod brain_away_tests;
