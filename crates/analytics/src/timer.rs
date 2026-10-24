//! 前台时长与活跃时长的纯逻辑计时器；时间由调用方注入，便于测试。

/// 距上次交互超过该窗口后不再计入活跃时间。
pub const IDLE_WINDOW_MS: u64 = 60_000;

#[derive(Debug, Default)]
pub struct ActiveTimer {
    foreground_since: Option<u64>,
    last_touch: u64,
    active_ms: u64,
}

impl ActiveTimer {
    pub fn is_foreground(&self) -> bool {
        self.foreground_since.is_some()
    }

    /// 进入前台；重复调用忽略。进入前台本身算一次交互。
    pub fn foreground(&mut self, now_ms: u64) {
        if self.foreground_since.is_none() {
            self.foreground_since = Some(now_ms);
            self.last_touch = now_ms;
            self.active_ms = 0;
        }
    }

    /// 记录一次键鼠 / 功能交互；后台时忽略。
    pub fn touch(&mut self, now_ms: u64) {
        if self.foreground_since.is_some() {
            self.credit(now_ms);
            self.last_touch = now_ms;
        }
    }

    /// 离开前台，返回 `(foreground_ms, active_ms)`；本就在后台则返回 `None`。
    pub fn background(&mut self, now_ms: u64) -> Option<(u64, u64)> {
        let since = self.foreground_since.take()?;
        self.credit(now_ms);
        Some((
            now_ms.saturating_sub(since),
            std::mem::take(&mut self.active_ms),
        ))
    }

    fn credit(&mut self, now_ms: u64) {
        self.active_ms += now_ms.saturating_sub(self.last_touch).min(IDLE_WINDOW_MS);
    }
}
