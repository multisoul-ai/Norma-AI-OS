use crate::{DelegationNotice, LiveError, Speaker, TranscriptFragment};
use std::collections::{BTreeMap, HashSet};
mod history;
mod observed;

#[derive(Clone, Debug, PartialEq)]
pub struct DelegationSnapshot {
    pub delegation_id: String,
    pub revision: u64,
    pub user_input: String,
    pub conversation: Vec<TranscriptFragment>,
    user_events: Vec<String>,
    offset_ms: f64,
    utterance_start_ms: f64,
    observed: bool,
    pub history_truncated: bool,
}

impl DelegationSnapshot {
    /// Stable identity of the first observed source event, independent of delegation and revision.
    pub fn intent_anchor(&self) -> &str {
        &self.user_events[0]
    }
    pub fn utterance_start_ms(&self) -> f64 {
        self.utterance_start_ms
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DelegationRequest {
    Pending,
    Ready(DelegationSnapshot),
    Duplicate {
        original_delegation_id: String,
        revision: u64,
    },
}

#[derive(Default)]
pub struct LiveContext {
    conversation: Vec<TranscriptFragment>,
    revision: u64,
    claimed: BTreeMap<u64, ClaimedInput>,
    settled: HashSet<String>,
    retained_bytes: usize,
    utterance_starts: Vec<f64>,
    retained_start_ms: f64,
    pending_boundary: Option<u64>,
    observed_suspended: bool,
    truncated_user_input: bool,
}

struct ClaimedInput {
    delegation_id: String,
    utterance_start_ms: f64,
    settled: bool,
}

impl LiveContext {
    /// Revoke old authorization immediately, before a delayed VAD segment confirms its start.
    pub fn suspend_user_input(&mut self) -> u64 {
        self.revision += 1;
        self.pending_boundary = Some(self.revision);
        self.observed_suspended = true;
        self.revision
    }

    pub fn confirm_user_utterance(
        &mut self,
        ticket: u64,
        start_ms: f64,
    ) -> Result<bool, LiveError> {
        if self.pending_boundary != Some(ticket) {
            return Ok(false);
        }
        if self
            .utterance_starts
            .last()
            .is_some_and(|start| start_ms <= *start)
        {
            return Ok(false);
        }
        self.begin_user_utterance(start_ms)?;
        self.pending_boundary = None;
        Ok(true)
    }

    /// Host-confirmed speech starts on the service session timeline.
    /// Upload sample counts alone do not establish this coordinate; never use packet arrival time.
    pub fn begin_user_utterance(&mut self, start_ms: f64) -> Result<(), LiveError> {
        if !start_ms.is_finite()
            || start_ms < self.retained_start_ms
            || self
                .utterance_starts
                .last()
                .is_some_and(|last| start_ms < *last)
        {
            return Err(LiveError::InvalidEvent);
        }
        if self.utterance_starts.last() == Some(&start_ms) {
            return Ok(());
        }
        if self.utterance_starts.len() >= 512 {
            self.prune_history();
        }
        if self.utterance_starts.len() >= 512 {
            return Err(LiveError::Capacity);
        }
        self.utterance_starts.push(start_ms);
        self.revision += 1;
        Ok(())
    }

    pub fn observe(&mut self, fragment: TranscriptFragment) -> Result<(), LiveError> {
        if fragment.event_id.is_empty()
            || fragment.event_id.len() > 256
            || fragment.delta.len() > 16_384
            || !fragment.start_ms.is_finite()
            || fragment.start_ms < self.retained_start_ms
            || !fragment.end_ms.is_finite()
            || fragment.end_ms < fragment.start_ms
        {
            return Err(LiveError::InvalidEvent);
        }
        if let Some(previous) = self
            .conversation
            .iter()
            .find(|part| part.event_id == fragment.event_id)
        {
            return if *previous == fragment {
                Ok(())
            } else {
                Err(LiveError::InvalidEvent)
            };
        }
        // Retire only older utterances, with an offset floor and in-flight claims pinned.
        if self.conversation.len() >= 512 || self.retained_bytes + fragment.delta.len() > 128 * 1024
        {
            self.prune_history();
        }
        if fragment.start_ms < self.retained_start_ms {
            return Err(LiveError::InvalidEvent);
        }
        if self.conversation.len() >= 512 || self.retained_bytes + fragment.delta.len() > 128 * 1024
        {
            return Err(LiveError::Capacity);
        }
        self.retained_bytes += fragment.delta.len();
        if fragment.speaker == Speaker::User {
            self.revision += 1;
            self.observed_suspended = false;
        }
        self.conversation.push(fragment);
        Ok(())
    }

    pub fn prepare(&self, notice: &DelegationNotice) -> DelegationRequest {
        if self.pending_boundary.is_some() {
            return DelegationRequest::Pending;
        }
        let Some(start) = self
            .utterance_starts
            .iter()
            .rev()
            .find(|start| **start <= notice.offset_ms)
            .copied()
        else {
            return DelegationRequest::Pending;
        };
        if self.conversation.iter().any(|part| {
            part.speaker == Speaker::User
                && part.start_ms <= notice.offset_ms
                && part.end_ms > notice.offset_ms
        }) {
            // Approximate spans cannot be split into reliably authorized words.
            return DelegationRequest::Pending;
        }
        let pending: Vec<_> = self
            .conversation
            .iter()
            .filter(|part| {
                part.speaker == Speaker::User
                    && part.start_ms >= start
                    && part.end_ms <= notice.offset_ms
                    && !self.settled.contains(&part.event_id)
            })
            .collect();
        let user_input: String = pending.iter().map(|part| part.delta.as_str()).collect();
        if user_input.trim().is_empty() {
            return DelegationRequest::Pending;
        }
        if let Some((revision, claim)) = self
            .claimed
            .iter()
            .find(|(_, claim)| !claim.settled && claim.utterance_start_ms == start)
        {
            return DelegationRequest::Duplicate {
                original_delegation_id: claim.delegation_id.clone(),
                revision: *revision,
            };
        }
        DelegationRequest::Ready(DelegationSnapshot {
            delegation_id: notice.id.clone(),
            revision: self.revision,
            user_input,
            conversation: self
                .conversation
                .iter()
                .filter(|part| part.end_ms <= notice.offset_ms)
                .cloned()
                .collect(),
            user_events: pending.iter().map(|part| part.event_id.clone()).collect(),
            offset_ms: notice.offset_ms,
            utterance_start_ms: start,
            observed: false,
            history_truncated: false,
        })
    }

    pub fn is_current(&self, snapshot: &DelegationSnapshot) -> bool {
        if snapshot.observed {
            return !self.observed_suspended
                && snapshot.revision == self.revision
                && snapshot.history_truncated == self.truncated_user_input
                && snapshot.user_events.iter().all(|id| {
                    !self.settled.contains(id)
                        && self.conversation.iter().any(|part| &part.event_id == id)
                });
        }
        self.pending_boundary.is_none()
            && snapshot.revision == self.revision
            && !self
                .utterance_starts
                .last()
                .is_some_and(|start| *start > snapshot.offset_ms)
            && !self
                .conversation
                .iter()
                .any(|part| part.speaker == Speaker::User && part.end_ms > snapshot.offset_ms)
            && snapshot
                .user_events
                .iter()
                .all(|id| !self.settled.contains(id))
    }

    pub fn claim(&mut self, snapshot: &DelegationSnapshot) -> bool {
        if !self.is_current(snapshot)
            || self.claimed.contains_key(&snapshot.revision)
            || self.claimed.values().any(|claim| {
                !claim.settled && claim.utterance_start_ms == snapshot.utterance_start_ms
            })
        {
            return false;
        }
        self.claimed.insert(
            snapshot.revision,
            ClaimedInput {
                delegation_id: snapshot.delegation_id.clone(),
                utterance_start_ms: snapshot.utterance_start_ms,
                settled: false,
            },
        );
        true
    }

    pub fn settle(&mut self, snapshot: &DelegationSnapshot) -> bool {
        if !self
            .claimed
            .get(&snapshot.revision)
            .is_some_and(|claim| claim.delegation_id == snapshot.delegation_id && !claim.settled)
            || snapshot
                .user_events
                .iter()
                .any(|id| self.settled.contains(id))
        {
            return false;
        }
        // Accepted effects consume their authorization even when later speech invalidates the reply.
        self.settled.extend(snapshot.user_events.iter().cloned());
        self.claimed
            .get_mut(&snapshot.revision)
            .expect("validated claim")
            .settled = true;
        if snapshot.observed && self.is_current_input_consumed() {
            self.truncated_user_input = false;
        }
        self.revision += 1;
        true
    }

    /// Only after the host has verified that no side effect was accepted or left unknown.
    pub fn release_unexecuted(&mut self, snapshot: &DelegationSnapshot) -> bool {
        if !self
            .claimed
            .get(&snapshot.revision)
            .is_some_and(|claim| claim.delegation_id == snapshot.delegation_id && !claim.settled)
        {
            return false;
        }
        self.claimed.remove(&snapshot.revision);
        self.revision += 1;
        true
    }
}
