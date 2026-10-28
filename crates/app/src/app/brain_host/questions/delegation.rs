use super::*;

/// User permission to choose an answer for exactly this original question, once.
pub(super) struct AnswerDelegation {
    pub(super) question: PendingQuestion,
    scope: ContextScope,
    deadline: Instant,
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
        self.brain
            .questions
            .delegated
            .retain(|grant| Instant::now() < grant.deadline);
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
            .position(|grant| grant.question.target == *target && Instant::now() < grant.deadline)
            .ok_or("question_not_authorized：请先明确委托当前问题")?;
        let mut permit = self.authorize_question_answer(target, answers)?;
        let grant = &self.brain.questions.delegated[index];
        if grant.question != permit.question || grant.scope != permit.scope {
            return Err("question_authorization_expired".into());
        }
        permit.deadline = grant.deadline;
        self.brain.questions.delegated.remove(index);
        Ok(permit)
    }

    pub(super) fn question_delegated(&self, target: &QuestionRef) -> bool {
        self.brain
            .questions
            .delegated
            .iter()
            .any(|grant| grant.question.target == *target && Instant::now() < grant.deadline)
    }

    pub(crate) fn delegate_overview_question(
        &mut self,
        area: &str,
        target: &QuestionRef,
        cx: &mut Context<Self>,
    ) {
        let result = self
            .with_overview_session(area, cx, |app, cx| {
                if app.brain.host.is_none() {
                    return Err("question_brain_unavailable".to_owned());
                }
                if app.authorize_question_delegation(target)? {
                    let question = app
                        .runtime
                        .question(target)
                        .ok_or("question_not_found")?
                        .question
                        .clone();
                    app.push_brain(brain::BrainEvent::QuestionRequested {
                        agent: app
                            .agent_announcement_name(target.node_id)
                            .unwrap_or_else(|| format!("{:?}", target.provider)),
                        question,
                        delegated: true,
                    });
                    cx.notify();
                }
                Ok(())
            })
            .unwrap_or_else(|| Err("question_project_closed".into()));
        self.brain
            .questions
            .ui_errors
            .retain(|(key, _)| key != target);
        if let Err(error) = result {
            self.brain.questions.ui_errors.push((target.clone(), error));
        }
        self.refresh_brain_overview(cx);
        cx.notify();
    }

    pub(crate) fn poll_brain_questions(&mut self) {
        self.brain
            .questions
            .prepared
            .retain(|(submission, _)| self.runtime.native_question_preparation_valid(submission));
        self.brain.questions.delegated.retain(|grant| {
            Instant::now() < grant.deadline
                && self
                    .runtime
                    .native_question_available(&grant.question.target)
        });
        self.brain.questions.suggestions.retain(|suggestion| {
            self.runtime
                .question(&suggestion.target)
                .is_some_and(|q| q.state.blocks_input())
        });
        let Some(host) = &self.brain.host else {
            return;
        };
        if self.brain.questions.announced_token != host.token {
            self.brain.questions.announced_token = host.token.clone();
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
                self.push_brain(brain::BrainEvent::QuestionRequested {
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
