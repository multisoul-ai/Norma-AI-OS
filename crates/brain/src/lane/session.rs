//! 幕僚任务会话的生命周期：空闲回收，以及随会话失效的任务 token。
use super::worker::Job;
use std::time::Duration;

/// 空闲轮询的结果。`Job` 表示自己的队列里又有活，继续处理。
pub enum Tick {
    Stay,
    Exit,
    Job(Job),
}

/// 任务会话的空闲回收：每轮空等都会调 `poll`（有人排队就驱逐）。
/// 空闲满 `after` 后调 `retire`；它返回 `Some(job)` 说明这时又来了新任务，
/// 返回 `None` 表示已从调度表里注销，线程可以退出。
pub struct Idle {
    pub after: Duration,
    pub retire: Box<dyn Fn() -> Option<Job> + Send>,
    pub poll: Option<Box<dyn Fn(bool) -> Tick + Send>>,
}

/// 会话的任务 token 随会话一起失效：被替换或线程退出时撤销。
pub(super) struct SessionToken {
    pub(super) tokens: Option<crate::staff::gate::StaffTokens>,
    pub(super) token: Option<String>,
}

impl SessionToken {
    /// 换成新会话的 token；上一把立刻撤销，不留到线程退出。
    pub(super) fn replace(&mut self, new: Option<String>) {
        if let (Some(tokens), Some(old)) = (&self.tokens, &self.token) {
            tokens.revoke(old);
        }
        self.token = new;
    }
}

impl Drop for SessionToken {
    fn drop(&mut self) {
        if let (Some(tokens), Some(token)) = (&self.tokens, &self.token) {
            tokens.revoke(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SessionToken;
    use crate::staff::gate::StaffTokens;

    /// 场景：会话被重建（轮数轮换、临时故障重试）时，旧会话的 token 必须马上失效。
    /// 数据构造：为任务 a 铸第一把 token 并挂在 SessionToken 上，再换成第二把；最后让它离开作用域。
    /// 预期结果：换的瞬间第一把失效、第二把有效；离开作用域后第二把也失效。
    #[test]
    fn replacing_the_session_token_revokes_the_previous_one() {
        let tokens = StaffTokens::default();
        let (first, second) = (tokens.mint("a"), tokens.mint("a"));
        {
            let mut session = SessionToken {
                tokens: Some(tokens.clone()),
                token: Some(first.clone()),
            };
            session.replace(Some(second.clone()));
            assert_eq!(tokens.task_of(&first), None, "check: old token revoked");
            assert_eq!(
                tokens.task_of(&second).as_deref(),
                Some("a"),
                "check: new token valid"
            );
        }
        assert_eq!(
            tokens.task_of(&second),
            None,
            "check: token revoked on drop"
        );
    }
}
