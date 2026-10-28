//! Retired foreground leases keep their original client and consumption identity.
use super::*;
impl Worker {
    pub(super) fn retire_waiting(&mut self) {
        self.remember_sends();
        for waiting in self.waiting.drain(..) {
            waiting.ticket.cancel();
            self.retired.push_back(waiting);
        }
        self.tickets
            .lock()
            .expect("question ownership poisoned")
            .clear();
    }
    pub(super) fn reconcile_retired(&mut self) {
        // One bounded point-read per tick; retired sessions cannot starve foreground discovery.
        let Some(waiting) = self.retired.pop_front() else {
            return;
        };
        if let Some(event) = waiting.closed_event() {
            self.emit(event);
        } else {
            self.retired.push_back(waiting);
        }
    }
}
impl Waiting {
    pub(super) fn closed_event(&self) -> Option<OpenCodeNativeEvent> {
        // Disappearance from pending only revokes eligibility, not evidence of consumption.
        self.ticket.cancel();
        let part = self.client.tool_part(&self.lease).ok()??;
        if self.lease.question().confirms_termination(&part) {
            let mut event = self.change(
                QuestionState::Cancelled,
                Some(part.to_string()),
                Some("opencode_question_terminated".into()),
            );
            if let OpenCodeNativeEvent::Changed { delivery_id, .. } = &mut event {
                *delivery_id = None;
            }
            return Some(event);
        }
        self.lease.question().consumed_answers(&part)?;
        let ours = self.accepted
            && self
                .reply
                .as_ref()
                .is_some_and(|r| self.lease.question().confirms_answer(&part, &r.answers));
        let state = if ours {
            QuestionState::Answered
        } else if self.uncertain {
            QuestionState::ConsumedUnattributed
        } else {
            QuestionState::AnsweredElsewhere
        };
        let mut event = self.change(state, Some(part.to_string()), None);
        if !ours {
            if let OpenCodeNativeEvent::Changed {
                delivery_id, error, ..
            } = &mut event
            {
                *delivery_id = None;
                if self.uncertain {
                    *error = Some("question_consumed_delivery_unknown".into());
                }
            }
        }
        Some(event)
    }
}
