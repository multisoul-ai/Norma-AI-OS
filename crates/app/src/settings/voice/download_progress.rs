// 4 MiB / 125 MiB/s（约 1 Gbps）≈ 32ms，因此即使高速下载也约束在 30fps；
// 644 MiB 推荐包最多产生约 644 / 4 = 161 次中间进度，而不是约 10,304 个 64 KiB 读取块。
const PROGRESS_STEP_BYTES: u64 = 4 * 1024 * 1024;

pub(super) struct DownloadProgressGate {
    total: u64,
    last_published: u64,
}

impl DownloadProgressGate {
    pub(super) fn new(total: u64) -> Self {
        Self {
            total,
            last_published: 0,
        }
    }

    pub(super) fn should_publish(&mut self, completed: u64) -> bool {
        let completed = self.visible_completed(completed);
        let reached_end = self.total > 0 && completed >= self.total;
        let reached_step = completed.saturating_sub(self.last_published) >= PROGRESS_STEP_BYTES;
        if !reached_end && !reached_step {
            return false;
        }
        self.last_published = completed;
        true
    }

    pub(super) fn should_publish_final(&mut self, completed: u64) -> bool {
        let completed = self.visible_completed(completed);
        if completed == self.last_published {
            return false;
        }
        self.last_published = completed;
        true
    }

    fn visible_completed(&self, completed: u64) -> u64 {
        if self.total == 0 {
            completed
        } else {
            completed.min(self.total)
        }
    }
}
