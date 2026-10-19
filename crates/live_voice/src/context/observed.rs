use super::*;

impl LiveContext {
    /// Prepare a semantic candidate from known evidence. Neither offset nor audio ACK is a fence.
    /// Tool authorization and pre-effect revision checks remain the host's responsibility.
    pub fn prepare_observed(&self, notice: &DelegationNotice) -> DelegationRequest {
        if self.observed_suspended {
            return DelegationRequest::Pending;
        }
        if let Some((revision, claim)) = self.claimed.iter().find(|(_, claim)| !claim.settled) {
            return DelegationRequest::Duplicate {
                original_delegation_id: claim.delegation_id.clone(),
                revision: *revision,
            };
        }
        let pending: Vec<_> = self
            .conversation
            .iter()
            .filter(|part| part.speaker == Speaker::User && !self.settled.contains(&part.event_id))
            .collect();
        let Some(first) = pending.first() else {
            return DelegationRequest::Pending;
        };
        let mut ordered = pending.clone();
        ordered.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
        let user_input: String = ordered.iter().map(|part| part.delta.as_str()).collect();
        if user_input.trim().is_empty() {
            return DelegationRequest::Pending;
        }
        let mut conversation = self.conversation.clone();
        conversation.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
        DelegationRequest::Ready(DelegationSnapshot {
            delegation_id: notice.id.clone(),
            revision: self.revision,
            user_input,
            conversation,
            user_events: pending.iter().map(|part| part.event_id.clone()).collect(),
            offset_ms: notice.offset_ms,
            utterance_start_ms: pending
                .iter()
                .map(|part| part.start_ms)
                .min_by(f64::total_cmp)
                .unwrap_or(first.start_ms),
            observed: true,
            history_truncated: self.truncated_user_input,
        })
    }

    pub(super) fn is_current_input_consumed(&self) -> bool {
        self.conversation
            .iter()
            .all(|part| part.speaker != Speaker::User || self.settled.contains(&part.event_id))
    }
}
