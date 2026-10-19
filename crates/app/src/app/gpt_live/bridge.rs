#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_bridge_tests.rs"]
mod tests;
use super::report::DelegatedReport;
use live_voice::{
    DelegationNotice, DelegationRequest, DelegationSnapshot, LiveContext, LiveError,
    TranscriptFragment,
};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

pub(super) struct DelegatedWork {
    pub snapshot: DelegationSnapshot,
    pub intent: String,
    pub cancellation: CancellationToken,
}

pub(super) struct LiveBridge {
    pub timeline: Arc<Mutex<LiveContext>>,
    call: String,
    call_cancellation: CancellationToken,
    pending: VecDeque<DelegationNotice>,
    active: Option<(DelegationNotice, DelegationSnapshot, CancellationToken)>,
}

impl LiveBridge {
    pub fn new(call: String, call_cancellation: CancellationToken) -> Self {
        Self {
            timeline: Arc::new(Mutex::new(LiveContext::default())),
            call,
            call_cancellation,
            pending: VecDeque::new(),
            active: None,
        }
    }

    pub fn transcript(&mut self, fragment: TranscriptFragment) -> Result<(), LiveError> {
        let mut timeline = self.timeline.lock().expect("Live context lock");
        timeline.observe(fragment)?;
        if let Some((_, snapshot, cancellation)) = &self.active {
            if !timeline.is_current(snapshot) {
                cancellation.cancel();
            }
        }
        Ok(())
    }

    pub fn speech_started(&mut self) -> u64 {
        self.cancel_planning();
        self.timeline
            .lock()
            .expect("Live context lock")
            .suspend_user_input()
    }

    pub fn delegation(&mut self, notice: DelegationNotice) -> Result<(), LiveError> {
        if self.pending.iter().any(|previous| previous.id == notice.id)
            || self
                .active
                .as_ref()
                .is_some_and(|(active, _, _)| active.id == notice.id)
        {
            return Ok(());
        }
        if self.pending.len() + usize::from(self.active.is_some()) >= 256 {
            return Err(LiveError::Capacity);
        }
        self.pending.push_back(notice);
        Ok(())
    }

    pub fn claim_next(&mut self) -> Option<DelegatedWork> {
        if self.active.is_some() || self.call_cancellation.is_cancelled() {
            return None;
        }
        let mut timeline = self.timeline.lock().expect("Live context lock");
        for index in 0..self.pending.len() {
            let snapshot = match timeline.prepare_observed(&self.pending[index]) {
                DelegationRequest::Ready(snapshot) => snapshot,
                _ => continue,
            };
            if !timeline.claim(&snapshot) {
                continue;
            }
            let notice = self.pending.remove(index).expect("queued delegation");
            let cancellation = self.call_cancellation.child_token();
            self.active = Some((notice, snapshot.clone(), cancellation.clone()));
            return Some(DelegatedWork {
                intent: format!(
                    "{}:input-{:x}",
                    self.call,
                    Sha256::digest(snapshot.intent_anchor().as_bytes())
                ),
                snapshot,
                cancellation,
            });
        }
        None
    }

    pub fn complete(&mut self, work: &DelegatedWork, report: DelegatedReport) -> Option<String> {
        if !self
            .active
            .as_ref()
            .is_some_and(|(_, snapshot, _)| snapshot == &work.snapshot)
        {
            return None;
        }
        let (notice, _, _) = self.active.take().expect("matching active delegation");
        let mut timeline = self.timeline.lock().expect("Live context lock");
        let current = timeline.is_current(&work.snapshot) && !work.cancellation.is_cancelled();
        // A receipt still in flight is a possible side effect, even with an empty completed list.
        if report.pending || report.effects || current {
            timeline.settle(&work.snapshot);
        } else {
            if timeline.release_unexecuted(&work.snapshot) && !self.call_cancellation.is_cancelled()
            {
                self.pending.push_front(notice);
            }
        }
        if !current {
            return None;
        }
        if report.receipts.is_empty() && work.snapshot.history_truncated {
            return Some("前文已超出上下文窗口，未执行操作。请重新完整说明本次请求。".into());
        }
        Some(report.message())
    }

    fn cancel_planning(&mut self) {
        if let Some((_, _, token)) = &self.active {
            token.cancel();
        }
    }
}

impl Drop for LiveBridge {
    fn drop(&mut self) {
        self.cancel_planning();
    }
}
