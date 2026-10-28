use super::*;

impl Runtime {
    /// A retained durable lease is valid while the same live submission remains unsent.
    pub fn native_question_preparation_valid(&self, submission: &QuestionSubmission) -> bool {
        self.validate_question_connection(submission.target())
            .is_ok()
            && self.questions.entries.iter().any(|entry| {
                entry.submission.as_ref() == Some(submission)
                    && matches!(
                        entry.snapshot.state,
                        QuestionState::Submitting | QuestionState::Failed
                    )
                    && entry.hook.as_ref().is_some_and(|hook| {
                        hook.status(std::time::Instant::now())
                            == norma::agent_hook::QuestionHookStatus::Pending
                    })
            })
    }

    /// A live host retains the unsent durable lease; the original Hook must still be waiting.
    pub fn retry_native_question_preparation(
        &mut self,
        submission: &QuestionSubmission,
    ) -> Result<(), &'static str> {
        if !self.native_question_available(submission.target()) {
            return Err("question_not_available");
        }
        let entry = self.question_submission_mut(submission)?;
        if entry.snapshot.state != QuestionState::Failed {
            return Err("question_not_failed");
        }
        entry.snapshot.state = QuestionState::Submitting;
        entry.snapshot.error = None;
        Ok(())
    }

    /// An old durable intent has no live send lease. Hold it for reconciliation, never retry IO.
    pub fn hold_prior_question_delivery(
        &mut self,
        submission: &QuestionSubmission,
        original_id: &str,
    ) -> Result<(), &'static str> {
        if original_id.trim().is_empty() {
            return Err("invalid_delivery_id");
        }
        let entry = self.question_submission_mut(submission)?;
        if entry.snapshot.state != QuestionState::Submitting {
            return Err("question_not_submitting");
        }
        entry.snapshot.state = QuestionState::Unknown;
        entry.snapshot.delivery_id = Some(original_id.into());
        entry.snapshot.error = Some("question_prior_delivery_requires_reconciliation".into());
        entry.submission = None;
        if let Some(hook) = &entry.hook {
            hook.cancel();
        }
        Ok(())
    }

    /// No provider side effect has begun. The original native question still owns the input.
    pub fn fail_question_preparation(
        &mut self,
        submission: &QuestionSubmission,
        reason: &str,
    ) -> Result<(), &'static str> {
        let entry = self.question_submission_mut(submission)?;
        if entry.snapshot.state != QuestionState::Submitting {
            return Err("question_not_submitting");
        }
        entry.snapshot.state = QuestionState::Failed;
        entry.snapshot.error = Some(reason.into());
        Ok(())
    }

    pub(super) fn question_submission_mut(
        &mut self,
        submission: &QuestionSubmission,
    ) -> Result<&mut Entry, &'static str> {
        self.questions
            .entries
            .iter_mut()
            .find(|entry| entry.submission.as_ref() == Some(submission))
            .ok_or("question_submission_changed")
    }

    /// Final foreground revalidation after durable intent and authorization checks, before IO.
    /// Once this succeeds a crash may have delivered bytes; recovery must reconcile, never resend.
    pub fn start_question_delivery(
        &mut self,
        submission: &QuestionSubmission,
    ) -> Result<(), &'static str> {
        self.validate_question_connection(&submission.target)?;
        let entry = self.question_submission_mut(submission)?;
        if entry.snapshot.state != QuestionState::Submitting {
            return Err("question_not_submitting");
        }
        entry.snapshot.state = QuestionState::Submitted;
        Ok(())
    }

    pub fn question_delivery_unknown(
        &mut self,
        submission: &QuestionSubmission,
        reason: &str,
    ) -> Result<(), &'static str> {
        let entry = self.question_submission_mut(submission)?;
        if !matches!(
            entry.snapshot.state,
            QuestionState::Submitted | QuestionState::Unknown
        ) {
            return Err("question_not_submitted");
        }
        entry.snapshot.state = QuestionState::Unknown;
        entry.snapshot.error = Some(reason.into());
        Ok(())
    }

    /// Provider adapters supply correlated tool evidence, not a generic status/resolved event.
    pub fn confirm_question_answer(
        &mut self,
        submission: &QuestionSubmission,
        answers: &[QuestionAnswer],
        evidence: &str,
    ) -> Result<(), &'static str> {
        self.validate_question_run(&submission.target)?;
        if evidence.trim().is_empty()
            || answers.len() != submission.answers.len()
            || !submission
                .answers
                .iter()
                .all(|answer| answers.contains(answer))
        {
            return Err("question_consumption_mismatch");
        }
        let entry = self.question_submission_mut(submission)?;
        if !matches!(
            entry.snapshot.state,
            QuestionState::Submitted | QuestionState::Unknown
        ) {
            return Err("question_not_submitted");
        }
        entry.snapshot.state = QuestionState::Answered;
        entry.snapshot.evidence = Some(evidence.into());
        entry.snapshot.error = None;
        self.refresh_question_projection(submission.target.node_id);
        Ok(())
    }

    pub(super) fn refresh_question_projection(&mut self, node: NodeId) {
        if !self.live.contains_key(&node) || self.document.node(node).is_none() {
            return;
        }
        let state = if self.has_pending_question(node) {
            crate::AgentTaskState::WaitingQuestion
        } else {
            self.questions
                .provider_states
                .get(&node)
                .cloned()
                .unwrap_or(crate::AgentTaskState::Ready)
        };
        self.events
            .update_agent_task_state(node, state, "question/updated".into());
    }
}
