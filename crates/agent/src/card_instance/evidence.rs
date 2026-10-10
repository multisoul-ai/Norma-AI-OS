use super::*;

impl CardInstanceCoordinator {
    /// A provider start binds exactly one submitted prompt. Ambiguous repeated text stays unbound.
    pub fn observe_start(
        &mut self,
        target: &WorkspaceCardTarget,
        text: &str,
        turn: CardInstanceTurn,
    ) -> bool {
        if !valid_turn(target, &turn) {
            return false;
        }
        if self
            .receipts
            .values()
            .any(|r| r.target == *target && r.bound_turn.as_ref() == Some(&turn))
        {
            return false;
        }
        let candidates: Vec<_> = self
            .order
            .iter()
            .filter(|id| {
                let r = &self.receipts[*id];
                r.target == *target
                    && r.stage == CardInstanceStage::Submitted
                    && r.tracking == CardInstanceTracking::Managed
                    && self.assignments[*id].text == text
            })
            .cloned()
            .collect();
        let [id] = candidates.as_slice() else {
            return false;
        };
        let receipt = self.receipts.get_mut(id).expect("candidate exists");
        receipt.stage = CardInstanceStage::Running;
        receipt.bound_turn = Some(turn);
        self.record(id, "provider_turn_started");
        true
    }

    /// Only the exact bound provider turn may conclude a task; card state and queue order are irrelevant.
    pub fn observe_terminal(
        &mut self,
        target: &WorkspaceCardTarget,
        turn: &CardInstanceTurn,
        stage: CardInstanceStage,
        result: Option<AgentTaskResult>,
    ) -> bool {
        if !valid_turn(target, turn)
            || !matches!(
                stage,
                CardInstanceStage::Succeeded
                    | CardInstanceStage::Failed
                    | CardInstanceStage::Interrupted
            )
        {
            return false;
        }
        let Some(id) = self
            .order
            .iter()
            .find(|id| {
                let r = &self.receipts[*id];
                r.target == *target
                    && matches!(
                        r.stage,
                        CardInstanceStage::Running | CardInstanceStage::OutcomeUnknown
                    )
                    && r.bound_turn.as_ref() == Some(turn)
            })
            .cloned()
        else {
            return false;
        };
        let receipt = self.receipts.get_mut(&id).expect("bound receipt exists");
        receipt.stage = stage;
        receipt.result = result;
        receipt.failure = None;
        self.record(&id, "provider_turn_terminal");
        true
    }
}

fn valid_turn(target: &WorkspaceCardTarget, turn: &CardInstanceTurn) -> bool {
    target.execution.run_id == turn.run_id
        && target.execution.session_id.as_deref() == Some(turn.session_id.as_str())
        && !turn.turn_id.is_empty()
        && turn.turn_id.len() <= 512
        && !turn.turn_id.chars().any(char::is_control)
}
