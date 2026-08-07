use std::collections::VecDeque;
use thiserror::Error;

pub const MAX_ACTIVE_SPEECH_CHARS: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpeechSegmentReceipt {
    sequence: u64,
    chars: usize,
}

impl SpeechSegmentReceipt {
    pub fn chars(self) -> usize {
        self.chars
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingSpeechSubmission<'a> {
    receipt: SpeechSegmentReceipt,
    text: &'a str,
}

impl<'a> PendingSpeechSubmission<'a> {
    pub fn receipt(self) -> SpeechSegmentReceipt {
        self.receipt
    }

    pub fn text(self) -> &'a str {
        self.text
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SpeechBacklogError {
    #[error(
        "speech backlog active capacity exceeded: active={active_chars}, incoming={incoming_chars}, capacity={capacity_chars}"
    )]
    CapacityExceeded {
        active_chars: usize,
        incoming_chars: usize,
        capacity_chars: usize,
    },
    #[error("speech backlog input is closed")]
    InputClosed,
    #[error("speech backlog was cancelled")]
    Cancelled,
    #[error("speech backlog submission did not match the oldest pending segment")]
    SubmissionMismatch,
    #[error("speech backlog acknowledgement did not match the oldest submitted segment")]
    AcknowledgementMismatch,
}

#[derive(Debug)]
struct SpeechSegment {
    receipt: SpeechSegmentReceipt,
    text: String,
}

#[derive(Debug)]
pub struct SpeechBacklog {
    pending: VecDeque<SpeechSegment>,
    submitted: VecDeque<SpeechSegmentReceipt>,
    pending_chars: usize,
    submitted_unconsumed_chars: usize,
    cumulative_accepted_chars: usize,
    cumulative_submitted_chars: usize,
    cumulative_consumed_chars: usize,
    active_chars_high_water: usize,
    next_sequence: u64,
    input_closed: bool,
    cancelled: bool,
    capacity_exceeded: bool,
}

impl Default for SpeechBacklog {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            submitted: VecDeque::new(),
            pending_chars: 0,
            submitted_unconsumed_chars: 0,
            cumulative_accepted_chars: 0,
            cumulative_submitted_chars: 0,
            cumulative_consumed_chars: 0,
            active_chars_high_water: 0,
            next_sequence: 1,
            input_closed: false,
            cancelled: false,
            capacity_exceeded: false,
        }
    }
}

impl SpeechBacklog {
    pub fn accept_batch(&mut self, segments: Vec<String>) -> Result<(), SpeechBacklogError> {
        if self.cancelled {
            return Err(SpeechBacklogError::Cancelled);
        }
        if self.input_closed {
            return Err(SpeechBacklogError::InputClosed);
        }
        let incoming_chars = segments
            .iter()
            .try_fold(0_usize, |total, segment| {
                total.checked_add(segment.chars().count())
            })
            .unwrap_or(usize::MAX);
        if self.active_chars().saturating_add(incoming_chars) > MAX_ACTIVE_SPEECH_CHARS {
            self.input_closed = true;
            self.capacity_exceeded = true;
            return Err(SpeechBacklogError::CapacityExceeded {
                active_chars: self.active_chars(),
                incoming_chars,
                capacity_chars: MAX_ACTIVE_SPEECH_CHARS,
            });
        }

        for text in segments {
            let chars = text.chars().count();
            let receipt = SpeechSegmentReceipt {
                sequence: self.next_sequence,
                chars,
            };
            self.next_sequence = self
                .next_sequence
                .checked_add(1)
                .expect("one reply cannot contain u64::MAX speech segments");
            self.pending.push_back(SpeechSegment { receipt, text });
        }
        self.pending_chars = self.pending_chars.saturating_add(incoming_chars);
        self.cumulative_accepted_chars = self
            .cumulative_accepted_chars
            .saturating_add(incoming_chars);
        self.active_chars_high_water = self.active_chars_high_water.max(self.active_chars());
        Ok(())
    }

    pub fn next_submission(&self) -> Option<PendingSpeechSubmission<'_>> {
        self.pending.front().map(|segment| PendingSpeechSubmission {
            receipt: segment.receipt,
            text: &segment.text,
        })
    }

    pub fn mark_submitted(
        &mut self,
        receipt: SpeechSegmentReceipt,
    ) -> Result<(), SpeechBacklogError> {
        if self.cancelled {
            return Err(SpeechBacklogError::Cancelled);
        }
        if self.pending.front().map(|segment| segment.receipt) != Some(receipt) {
            return Err(SpeechBacklogError::SubmissionMismatch);
        }
        let segment = self
            .pending
            .pop_front()
            .expect("matching pending receipt proves a front segment exists");
        self.pending_chars = self.pending_chars.saturating_sub(receipt.chars);
        self.submitted_unconsumed_chars = self
            .submitted_unconsumed_chars
            .saturating_add(receipt.chars);
        self.cumulative_submitted_chars = self
            .cumulative_submitted_chars
            .saturating_add(receipt.chars);
        self.submitted.push_back(segment.receipt);
        Ok(())
    }

    pub fn acknowledge_consumed(
        &mut self,
        receipt: SpeechSegmentReceipt,
    ) -> Result<(), SpeechBacklogError> {
        if self.cancelled {
            return Err(SpeechBacklogError::Cancelled);
        }
        if self.submitted.front().copied() != Some(receipt) {
            return Err(SpeechBacklogError::AcknowledgementMismatch);
        }
        self.submitted.pop_front();
        self.submitted_unconsumed_chars = self
            .submitted_unconsumed_chars
            .saturating_sub(receipt.chars);
        self.cumulative_consumed_chars =
            self.cumulative_consumed_chars.saturating_add(receipt.chars);
        Ok(())
    }

    pub fn close_input(&mut self) {
        self.input_closed = true;
    }

    pub fn cancel(&mut self) {
        self.pending.clear();
        self.submitted.clear();
        self.pending_chars = 0;
        self.submitted_unconsumed_chars = 0;
        self.input_closed = true;
        self.cancelled = true;
    }

    pub fn active_chars(&self) -> usize {
        self.pending_chars
            .saturating_add(self.submitted_unconsumed_chars)
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.submitted.is_empty()
    }

    pub fn ready_to_finish(&self) -> bool {
        self.input_closed && !self.cancelled && self.is_empty()
    }

    pub fn cumulative_accepted_chars(&self) -> usize {
        self.cumulative_accepted_chars
    }

    pub fn cumulative_submitted_chars(&self) -> usize {
        self.cumulative_submitted_chars
    }

    pub fn cumulative_consumed_chars(&self) -> usize {
        self.cumulative_consumed_chars
    }

    pub fn active_chars_high_water(&self) -> usize {
        self.active_chars_high_water
    }

    pub fn capacity_exceeded(&self) -> bool {
        self.capacity_exceeded
    }
}
