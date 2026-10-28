use super::*;
use crate::app::OverviewQuestion;

impl MultisoulApp {
    /// Revoke an unconsumed delegation before a user's manual submission is validated.
    pub(crate) fn revoke_overview_question_delegation(
        &mut self,
        area: &str,
        target: &QuestionRef,
        cx: &mut Context<Self>,
    ) {
        self.with_overview_session(area, cx, |app, cx| {
            app.brain
                .questions
                .delegated
                .retain(|grant| grant.question.target != *target);
            cx.notify();
        });
    }

    /// An enabled button can belong to an old frame. Decide in the original owning session.
    pub(crate) fn prepare_overview_question_edit(
        &mut self,
        area: &str,
        target: &QuestionRef,
        cx: &mut Context<Self>,
    ) -> bool {
        let result = self
            .with_overview_session(area, cx, |app, cx| {
                app.question_context(target)?;
                if !app.runtime.native_question_available(target) {
                    return Err("question_not_available".to_owned());
                }
                app.brain
                    .questions
                    .delegated
                    .retain(|grant| grant.question.target != *target);
                cx.notify();
                Ok(())
            })
            .unwrap_or_else(|| Err("question_project_closed".into()));
        self.brain
            .questions
            .ui_errors
            .retain(|(key, _)| key != target);
        let available = result.is_ok();
        if let Err(error) = result {
            self.brain.questions.ui_errors.push((target.clone(), error));
        }
        self.refresh_brain_overview(cx);
        cx.notify();
        available
    }

    pub(crate) fn release_overview_question(
        &mut self,
        area: &str,
        target: &QuestionRef,
        cx: &mut Context<Self>,
    ) -> bool {
        let result = self
            .with_overview_session(area, cx, |app, cx| {
                app.runtime.release_question_to_native(target)?;
                app.brain
                    .questions
                    .prepared
                    .retain(|(submission, _)| submission.target() != target);
                app.brain
                    .questions
                    .delegated
                    .retain(|permit| permit.question.target != *target);
                cx.notify();
                Ok::<(), &'static str>(())
            })
            .unwrap_or(Err("question_project_closed"));
        self.brain
            .questions
            .ui_errors
            .retain(|(key, _)| key != target);
        if let Err(error) = result {
            self.brain
                .questions
                .ui_errors
                .push((target.clone(), error.into()));
        }
        self.refresh_brain_overview(cx);
        cx.notify();
        result.is_ok()
    }

    pub(crate) fn question_overview_rows(&self, area: &str) -> Vec<OverviewQuestion> {
        self.runtime
            .questions()
            .into_iter()
            .filter(|snapshot| {
                let target = &snapshot.question.target;
                snapshot.state.blocks_input() && self.runtime.question_is_current(target)
            })
            .map(|snapshot| {
                let target = &snapshot.question.target;
                OverviewQuestion {
                    area_id: area.into(),
                    agent: self
                        .runtime
                        .document
                        .node(target.node_id)
                        .map(|node| node.kind.display_title())
                        .unwrap_or_else(|| format!("{:?}", target.provider)),
                    available: self.runtime.native_question_available(target),
                    delegated: self.question_delegated(target),
                    can_delegate: self.brain.host.is_some()
                        && !snapshot.question.items.iter().any(|item| item.secret),
                    suggestion: self
                        .brain
                        .questions
                        .suggestions
                        .iter()
                        .find(|item| {
                            item.target == *target
                                && !snapshot.question.items.iter().any(|item| item.secret)
                        })
                        .cloned(),
                    snapshot,
                }
            })
            .collect()
    }

    pub(crate) fn question_action_error(&self, target: &QuestionRef) -> Option<&str> {
        self.brain
            .questions
            .ui_errors
            .iter()
            .find(|(key, _)| key == target)
            .map(|(_, error)| error.as_str())
    }

    pub(crate) fn submit_overview_question(
        &mut self,
        area: &str,
        target: QuestionRef,
        answers: Vec<QuestionAnswer>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        self.revoke_overview_question_delegation(area, &target, cx);
        self.brain
            .questions
            .ui_errors
            .retain(|(key, _)| key != &target);
        let result = self
            .with_overview_session(area, cx, |app, cx| {
                let permit = app.authorize_question_answer(&target, answers)?;
                app.begin_authorized_answer(permit, cx)
            })
            .unwrap_or_else(|| Err("question_project_closed".into()));
        self.refresh_brain_overview(cx);
        cx.notify();
        match result {
            Ok(work) => cx.spawn(async move |this, cx| {
                work.await;
                let _ = this.update(cx, |app, cx| app.refresh_brain_overview(cx));
            }),
            Err(error) => {
                if self.brain.questions.ui_errors.len() >= 32 {
                    self.brain.questions.ui_errors.remove(0);
                }
                self.brain.questions.ui_errors.push((target, error));
                Task::ready(())
            }
        }
    }
}
