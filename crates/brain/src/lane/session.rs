//! 幕僚任务会话的生命周期：空闲回收，以及随会话失效的任务 token。
use super::worker::Job;
use std::time::Duration;

/// 任务会话的空闲回收：空闲满 `after` 后调 `retire`；它返回 `Some(job)` 说明
/// 这时又来了新任务，继续处理；返回 `None` 表示已从调度表里注销，线程可以退出。
pub struct Idle {
    pub after: Duration,
    pub retire: Box<dyn Fn() -> Option<Job> + Send>,
}

/// 会话的任务 token 随会话一起失效：被替换或线程退出时撤销。
pub(super) struct SessionToken {
    pub(super) tokens: Option<crate::staff::gate::StaffTokens>,
    pub(super) token: Option<String>,
}

impl Drop for SessionToken {
    fn drop(&mut self) {
        if let (Some(tokens), Some(token)) = (&self.tokens, &self.token) {
            tokens.revoke(token);
        }
    }
}
