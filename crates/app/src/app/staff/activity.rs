//! Transient queue bookkeeping only. `Task.messages` (including the running `StaffReply`)
//! is the single authority for what the conversation shows.
use brain::event::TaskUpdate;

#[derive(Default)]
pub(crate) struct TaskActivity {
    pub turn: u64,
    /// 已入队、还没开始的轮次数。
    pub queued: usize,
    /// 卡在 Claude 的 4 会话上限上，面板显示「排队中」。
    pub session_wait: bool,
}

impl TaskActivity {
    pub fn apply(&mut self, turn: u64, update: TaskUpdate) {
        match update {
            TaskUpdate::Queued => self.queued += 1,
            TaskUpdate::SessionWait => self.session_wait = true,
            TaskUpdate::Started if turn > self.turn => {
                self.turn = turn;
                self.queued = self.queued.saturating_sub(1);
                self.session_wait = false;
            }
            _ => {}
        }
    }

    /// 大脑服务被释放或重启：排队中的轮次不会再开始。
    pub fn interrupt(&mut self) {
        self.queued = 0;
        self.session_wait = false;
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/staff_activity_tests.rs"]
mod tests;
