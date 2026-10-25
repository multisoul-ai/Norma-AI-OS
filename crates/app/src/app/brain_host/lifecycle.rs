//! 服务重启后保留状态接收通道，避免迟到的拒绝、错误或结束事件消失。
use super::{activity_sink::Pending, BrainHostState};
use brain::BrainStatus;
use std::sync::mpsc::{self, TryRecvError};

impl BrainHostState {
    pub(super) fn retire_host(&mut self) {
        if let Some(mut host) = self.host.take() {
            let (_, empty) = mpsc::channel();
            self.retiring
                .push(std::mem::replace(&mut host.status, empty));
            // host Drop 只负责后台 shutdown；旧 token 已立即失效。
            self.last_turn.clear();
        }
    }

    pub(super) fn drain_retired(&mut self) {
        let outbox = &self.activity_outbox;
        self.retiring.retain(|rx| loop {
            match rx.try_recv() {
                Ok(BrainStatus::Trace(entry)) => outbox.push(Pending::Trace(entry)),
                // 旧服务的运行状态不能触发新服务的审批转人工，也不能覆盖动作来源。
                Ok(_) => {}
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => return false,
            }
        });
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_host_lifecycle_tests.rs"]
mod tests;
