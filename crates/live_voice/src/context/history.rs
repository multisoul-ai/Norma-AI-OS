use super::*;

impl LiveContext {
    pub(super) fn prune_history(&mut self) {
        // Manual boundaries retain eight utterances; Live retains a bounded evidence window.
        let mut floor = self
            .utterance_starts
            .get(self.utterance_starts.len().saturating_sub(8))
            .copied()
            .unwrap_or_else(|| {
                let mut bytes = 0;
                self.conversation
                    .iter()
                    .rev()
                    .take(128)
                    .take_while(|part| {
                        bytes += part.delta.len();
                        bytes <= 64 * 1024
                    })
                    .map(|part| part.start_ms)
                    .min_by(f64::total_cmp)
                    .unwrap_or(0.0)
            });
        for claim in self.claimed.values().filter(|claim| !claim.settled) {
            floor = floor.min(claim.utterance_start_ms);
        }
        if floor <= self.retained_start_ms {
            return;
        }
        self.retained_start_ms = floor;
        self.truncated_user_input |= self.conversation.iter().any(|part| {
            part.start_ms < floor
                && part.speaker == Speaker::User
                && !self.settled.contains(&part.event_id)
        });
        self.conversation.retain(|part| part.start_ms >= floor);
        self.utterance_starts.retain(|start| *start >= floor);
        self.claimed
            .retain(|_, claim| claim.utterance_start_ms >= floor);
        let retained: HashSet<_> = self
            .conversation
            .iter()
            .map(|part| part.event_id.as_str())
            .collect();
        self.settled.retain(|id| retained.contains(id.as_str()));
        self.retained_bytes = self.conversation.iter().map(|part| part.delta.len()).sum();
    }
}
