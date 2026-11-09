use super::*;

/// User permission to choose an answer for exactly this original question, once.
pub(super) struct AnswerDelegation {
    pub(super) question: PendingQuestion,
    scope: ContextScope,
    deadline: Instant,
    pub(super) cancellation: Option<tokio_util::sync::CancellationToken>,
}

impl AnswerDelegation {
    pub(super) fn valid(&self) -> bool {
        Instant::now() < self.deadline
            && self
                .cancellation
                .as_ref()
                .is_none_or(|token| !token.is_cancelled())
    }
}

impl MultisoulApp {
    pub(crate) fn authorize_question_delegation(
        &mut self,
        target: &QuestionRef,
    ) -> Result<bool, String> {
        let service = self.question_context(target)?;
        if !self.runtime.native_question_available(target) {
            return Err("question_not_available".into());
        }
        self.brain.questions.delegated.retain(|grant| grant.valid());
        if self
            .brain
            .questions
            .delegated
            .iter()
            .any(|grant| grant.question.target == *target)
        {
            return Ok(false);
        }
        let question = self
            .runtime
            .question(target)
            .ok_or("question_not_found")?
            .question
            .clone();
        if question.items.iter().any(|item| item.secret) {
            return Err("question_requires_private_answer".into());
        }
        self.brain.questions.delegated.push(AnswerDelegation {
            question,
            scope: service.scope().clone(),
            deadline: Instant::now() + Duration::from_secs(120),
            cancellation: None,
        });
        Ok(true)
    }

    pub(super) fn consume_question_delegation(
        &mut self,
        target: &QuestionRef,
        answers: Vec<QuestionAnswer>,
    ) -> Result<AuthorizedAnswer, String> {
        let index = self
            .brain
            .questions
            .delegated
            .iter()
            .position(|grant| grant.question.target == *target && grant.valid())
            .ok_or("question_not_authorized：请先明确委托当前问题")?;
        let mut permit = self.authorize_question_answer(target, answers)?;
        let grant = &self.brain.questions.delegated[index];
        if grant.question != permit.question || grant.scope != permit.scope {
            return Err("question_authorization_expired".into());
        }
        permit.deadline = grant.deadline;
        permit.cancellation = grant.cancellation.clone();
        self.brain.questions.delegated.remove(index);
        Ok(permit)
    }

    pub(super) fn question_delegated(&self, target: &QuestionRef) -> bool {
        self.brain
            .questions
            .delegated
            .iter()
            .any(|grant| grant.question.target == *target && grant.valid())
    }

    pub(crate) fn poll_brain_questions(&mut self) {
        self.brain
            .questions
            .prepared
            .retain(|(submission, _)| self.runtime.native_question_preparation_valid(submission));
        self.brain.questions.delegated.retain(|grant| {
            grant.valid()
                && self
                    .runtime
                    .native_question_available(&grant.question.target)
        });
        self.brain.questions.suggestions.retain(|suggestion| {
            self.runtime
                .question(&suggestion.target)
                .is_some_and(|q| q.state.blocks_input())
        });
        let Some(token) = self.brain.token() else {
            return;
        };
        if self.brain.questions.announced_token != token {
            self.brain.questions.announced_token = token;
            self.brain.questions.announced.clear();
        }
        self.brain.questions.announced.retain(|target| {
            self.runtime
                .question(target)
                .is_some_and(|q| q.state.blocks_input())
        });
        for snapshot in self.runtime.questions() {
            let question = snapshot.question;
            if self.runtime.native_question_available(&question.target)
                && !question.items.iter().any(|item| item.secret)
                && !self.brain.questions.announced.contains(&question.target)
            {
                let delegated = self.question_delegated(&question.target);
                self.brain.questions.announced.push(question.target.clone());
                self.push_brain(brain::BrainEventKind::QuestionRequested {
                    agent: self
                        .agent_announcement_name(question.target.node_id)
                        .unwrap_or_else(|| format!("{:?}", question.target.provider)),
                    question,
                    delegated,
                });
            }
        }
    }
}
